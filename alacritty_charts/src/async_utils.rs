//! Loads prometheus metrics every now and then and displays stats
use crate::prometheus;
use crate::SizeInfo;
use crate::TimeSeriesChart;
use crate::TimeSeriesSource;
use futures::future::lazy;
use futures::sync::{mpsc, oneshot};
use log::*;
use std::time::{Duration, Instant};
use tokio::prelude::*;
use tokio::runtime::current_thread;
use tokio::timer::Interval;

// TODO:
// - Add color fetch
#[derive(Debug, Clone)]
pub struct MetricRequest {
    pub pull_interval: u64,
    pub source_url: String,
    pub chart_index: usize,  // For Vec<TimeSeriesChart>
    pub series_index: usize, // For Vec<TimeSeriesSource>
    pub data: Option<prometheus::HTTPResponse>,
    pub capacity: usize, // This maps to the time range in seconds to query.
}

/// `AsyncChartTask` contains message types that async_coordinator can work on
#[derive(Debug)]
pub enum AsyncChartTask {
    LoadResponse(MetricRequest),
    SendMetricsOpenGLData(usize, usize, oneshot::Sender<Vec<f32>>),
    SendDecorationsOpenGLData(usize, usize, oneshot::Sender<Vec<f32>>),
    ChangeDisplaySize(f32, f32, f32, f32, oneshot::Sender<bool>),
    SendLastUpdatedEpoch(oneshot::Sender<u64>),
    // Maybe add CloudWatch/etc
}

/// Sends a request to the async_coordinator to get the latest update epoch of all
/// the charts
pub fn get_last_updated_chart_epoch(
    charts_tx: mpsc::Sender<AsyncChartTask>,
    tokio_handle: current_thread::Handle,
) -> u64 {
    let (chart_tx, chart_rx) = oneshot::channel();
    let get_latest_update_epoch = charts_tx
        .send(AsyncChartTask::SendLastUpdatedEpoch(chart_tx))
        .map_err(|e| error!("Sending SendLastUpdatedEpoch Task: err={:?}", e))
        .and_then(move |_res| {
            debug!("Sent Request for SendLastUpdatedEpoch");
            Ok(())
        });
    tokio_handle
        .spawn(lazy(move || get_latest_update_epoch))
        .expect("Unable to queue async task for get_latest_update_epoch");
    let chart_rx = chart_rx.map(|x| x);
    match chart_rx.wait() {
        Ok(data) => {
            debug!("Got response from SendLastUpdatedEpoch Task: {:?}", data);
            data
        },
        Err(err) => {
            error!("Error response from SendLastUpdatedEpoch Task: {:?}", err);
            0u64
        },
    }
}

/// `send_last_updated_epoch` returns the max of all the charts in an array
pub fn send_last_updated_epoch(charts: &[TimeSeriesChart], channel: oneshot::Sender<u64>) {
    match channel.send(charts.iter().map(|x| x.last_updated).max().unwrap_or_else(|| 0u64)) {
        Ok(()) => {
            debug!(
                "send_last_updated_epoch: oneshot::message sent with payload {}",
                charts.iter().map(|x| x.last_updated).max().unwrap_or_else(|| 0u64)
            );
        },
        Err(err) => error!("send_last_updated_epoch: Error sending: {:?}", err),
    };
}

/// `load_http_response` is called by async_coordinator when a task of type
/// LoadResponse is received
pub fn load_http_response(
    charts: &mut Vec<TimeSeriesChart>,
    response: MetricRequest,
    size: SizeInfo,
) {
    if let Some(data) = response.data {
        let mut ok_records = 0;
        if response.chart_index < charts.len()
            && response.series_index < charts[response.chart_index].sources.len()
        {
            if let TimeSeriesSource::PrometheusTimeSeries(ref mut prom) =
                charts[response.chart_index].sources[response.series_index]
            {
                match prom.load_prometheus_response(data) {
                    Ok(num_records) => {
                        info!(
                            "Loaded {} records from {} into TimeSeries",
                            num_records, response.source_url
                        );
                        ok_records = num_records;
                    },
                    Err(err) => {
                        debug!("Error Loading {} into TimeSeries: {:?}", response.source_url, err);
                    },
                }
                debug!(
                    "After loading. TimeSeries is: {:?}",
                    charts[response.chart_index].sources[response.series_index]
                );
            }
            charts[response.chart_index].update_series_opengl_vecs(response.series_index, size);
        }
        for chart in charts {
            // Update the loaded item counters
            for series in &mut chart.sources {
                if let TimeSeriesSource::AsyncLoadedItems(ref mut loaded) = series {
                    loaded.series.push_current_epoch(ok_records as f64);
                }
            }
        }
    }
}

/// `send_metrics_opengl_vecs` is called by async_coordinator when an task of
/// type SendMetricsOpenGLData is received, it should contain the chart index
/// to represent as OpenGL vertices, it returns data through the channel parameter
pub fn send_metrics_opengl_vecs(
    charts: &[TimeSeriesChart],
    chart_index: usize,
    data_index: usize,
    channel: oneshot::Sender<Vec<f32>>,
) {
    debug!("send_metrics_opengl_vecs for chart_index: {}", chart_index);
    match channel.send(
        if chart_index >= charts.len() || data_index >= charts[chart_index].opengl_vecs.len() {
            vec![]
        } else {
            charts[chart_index].get_deduped_opengl_vecs(data_index)
        },
    ) {
        Ok(()) => {
            debug!("send_metrics_opengl_vecs: oneshot::message sent for {}", chart_index);
        },
        Err(err) => error!("send_metrics_opengl_vecs: Error sending: {:?}", err),
    };
}

/// `send_decorations_opengl_vecs` is called by async_coordinator when an task of
/// type SendDecorationsOpenGLData is received, it should contain the chart index
/// to represent as OpenGL vertices, it returns data through the channel parameter
pub fn send_decorations_opengl_vecs(
    charts: &[TimeSeriesChart],
    chart_index: usize,
    data_index: usize,
    channel: oneshot::Sender<Vec<f32>>,
) {
    debug!("send_decorations_vecs for chart_index: {}", chart_index);
    match channel.send(
        if chart_index >= charts.len() || data_index >= charts[chart_index].decorations.len() {
            vec![]
        } else {
            debug!(
                "send_decorations_opengl_vecs Sending vertices: {:?}",
                charts[chart_index].decorations[data_index].opengl_vertices()
            );
            charts[chart_index].decorations[data_index].opengl_vertices()
        },
    ) {
        Ok(()) => {
            debug!(
                "send_decorations_opengl_vecs: oneshot::message sent for index: {}",
                chart_index
            );
        },
        Err(err) => error!("send_decorations_opengl_vecs: Error sending: {:?}", err),
    };
}
/// `change_display_size` handles changes to the Display
/// It is debatable that we need to handle this message or return
/// anything, so we'll just return a true ACK, the charts are updated
/// after the size changes, potentially could be slow and we should delay
/// until the size is stabilized.
pub fn change_display_size(
    charts: &mut Vec<TimeSeriesChart>,
    size: &mut SizeInfo,
    height: f32,
    width: f32,
    padding_y: f32,
    padding_x: f32,
    channel: oneshot::Sender<bool>,
) {
    debug!(
        "change_display_size for height: {}, width: {}, padding_y: {}, padding_x: {}",
        height, width, padding_y, padding_x
    );
    size.height = height;
    size.width = width;
    size.padding_y = padding_y;
    size.padding_x = padding_x;
    for chart in charts {
        // Update the OpenGL representation when the display changes
        chart.update_all_series_opengl_vecs(*size);
    }
    match channel.send(true) {
        Ok(()) => {
            debug!("change_display_size: Sent reply back to resize notifier, new size: {:?}", size)
        },
        Err(err) => error!("change_display_size: Error sending: {:?}", err),
    };
}

/// `async_coordinator` receives messages from the tasks about data loaded from
/// the network, it owns the charts data, and may draw the Charts to OpenGL
pub fn async_coordinator(
    rx: mpsc::Receiver<AsyncChartTask>,
    mut charts: Vec<TimeSeriesChart>,
    height: f32,
    width: f32,
    padding_y: f32,
    padding_x: f32,
) -> impl Future<Item = (), Error = ()> {
    debug!(
        "async_coordinator: Starting, height: {}, width: {}, padding_y: {}, padding_x {}",
        height, width, padding_y, padding_x
    );
    for chart in &mut charts {
        // Update the loaded item counters
        debug!("Finishing setup for sources in chart: '{}'", chart.name);
        for series in &mut chart.sources {
            series.init();
        }
    }
    let mut size = SizeInfo { height, width, padding_y, padding_x, ..SizeInfo::default() };
    rx.for_each(move |message| {
        debug!("async_coordinator: message: {:?}", message);
        match message {
            AsyncChartTask::LoadResponse(req) => load_http_response(&mut charts, req, size),
            AsyncChartTask::SendMetricsOpenGLData(chart_index, data_index, channel) => {
                send_metrics_opengl_vecs(&charts, chart_index, data_index, channel);
            },
            AsyncChartTask::SendDecorationsOpenGLData(chart_index, data_index, channel) => {
                send_decorations_opengl_vecs(&charts, chart_index, data_index, channel);
            },
            AsyncChartTask::ChangeDisplaySize(height, width, padding_y, padding_x, channel) => {
                change_display_size(
                    &mut charts,
                    &mut size,
                    height,
                    width,
                    padding_y,
                    padding_x,
                    channel,
                );
            },
            AsyncChartTask::SendLastUpdatedEpoch(channel) => {
                send_last_updated_epoch(&charts, channel);
            },
        };
        Ok(())
    })
}

/// `fetch_prometheus_response` gets data from prometheus and once data is ready
/// it sends the results to the coordinator.
fn fetch_prometheus_response(
    item: MetricRequest,
    tx: mpsc::Sender<AsyncChartTask>,
) -> impl Future<Item = (), Error = ()> {
    debug!("fetch_prometheus_response: Starting");
    let url = prometheus::PrometheusTimeSeries::prepare_url(&item.source_url, item.capacity as u64)
        .unwrap();
    prometheus::get_from_prometheus(url.clone())
        .timeout(Duration::from_secs(item.pull_interval))
        .map_err(|e| error!("get_from_prometheus; err={:?}", e))
        .and_then(move |value| {
            debug!("Got prometheus raw value={:?}", value);
            let res = prometheus::parse_json(&value);
            debug!("Parsed JSON to res={:?}", res);
            tx.send(AsyncChartTask::LoadResponse(MetricRequest {
                source_url: item.source_url.clone(),
                chart_index: item.chart_index,
                series_index: item.series_index,
                pull_interval: item.pull_interval,
                data: res.clone(),
                capacity: item.capacity,
            }))
            .map_err(|e| {
                error!("fetch_prometheus_response: send data back to coordinator; err={:?}", e)
            })
            .and_then(|res| {
                debug!("fetch_prometheus_response: res={:?}", res);
                Ok(())
            })
        })
        .map_err(|e| error!("Sending result to coordinator; err={:?}", e))
}

/// `spawn_charts_intervals` iterates over the charts and sources
/// and, if PrometheusTimeSeries it would call the spawn_datasource_interval_polls on it,
/// that would be constantly loading data asynchronously.
pub fn spawn_charts_intervals(
    charts: Vec<TimeSeriesChart>,
    charts_tx: mpsc::Sender<AsyncChartTask>,
    tokio_handle: current_thread::Handle,
) {
    let mut chart_index = 0usize;
    for chart in charts {
        debug!("Loading chart series with name: '{}'", chart.name);
        let mut series_index = 0usize;
        for series in chart.sources {
            if let TimeSeriesSource::PrometheusTimeSeries(ref prom) = series {
                debug!(" - Found time_series, adding interval run");
                let data_request = MetricRequest {
                    source_url: prom.source.clone(),
                    pull_interval: prom.pull_interval as u64,
                    chart_index,
                    series_index,
                    capacity: prom.series.metrics_capacity,
                    data: None,
                };
                let charts_tx = charts_tx.clone();
                tokio_handle
                    .spawn(lazy(move || spawn_datasource_interval_polls(&data_request, charts_tx)))
                    .expect("Got error spawning datasource internal polls");
            }
            series_index += 1;
        }
        chart_index += 1;
    }
}
/// `spawn_datasource_interval_polls` creates intervals for each series requested
/// Each series will have to reply to a mspc tx with the data
pub fn spawn_datasource_interval_polls(
    item: &MetricRequest,
    tx: mpsc::Sender<AsyncChartTask>,
) -> impl Future<Item = (), Error = ()> {
    debug!("spawn_datasource_interval_polls: Starting for item={:?}", item);
    Interval::new(Instant::now(), Duration::from_secs(item.pull_interval))
        //.take(10) //  Test 10 times first
        .map_err(|e| panic!("interval errored; err={:?}", e))
        .fold(
            MetricRequest {
                source_url: item.source_url.clone(),
                chart_index: item.chart_index,
                series_index: item.series_index,
                pull_interval: item.pull_interval,
                data: None,
                capacity: item.capacity,
            },
            move |async_metric_item, instant| {
                debug!(
                    "Interval triggered for {:?} at instant={:?}",
                    async_metric_item.source_url, instant
                );
                fetch_prometheus_response(async_metric_item.clone(), tx.clone()).and_then(|res| {
                    debug!("Got response {:?}", res);
                    Ok(async_metric_item)
                })
            },
            )
        .map(|_| ())
}

/// `get_metric_opengl_vecs` generates a oneshot::channel to communicate
/// with the async coordinator and request the vectors of the metric_data
/// or the decorations vertices
pub fn get_metric_opengl_vecs(
    charts_tx: mpsc::Sender<AsyncChartTask>,
    chart_idx: usize,
    series_idx: usize,
    request_type: &'static str,
    tokio_handle: current_thread::Handle,
) -> Vec<f32> {
    let (opengl_tx, opengl_rx) = oneshot::channel();
    let get_opengl_task = charts_tx
        .clone()
        .send(if request_type == "metric_data" {
            AsyncChartTask::SendMetricsOpenGLData(chart_idx, series_idx, opengl_tx)
        } else {
            AsyncChartTask::SendDecorationsOpenGLData(chart_idx, series_idx, opengl_tx)
        })
        .map_err(|e| error!("Sending SendMetricsOpenGL Task: err={:?}", e))
        .and_then(move |_res| {
            debug!(
                "Sent Request for SendMetricsOpenGL Task for chart index: {}, series: {}",
                chart_idx, series_idx
            );
            Ok(())
        });
    tokio_handle.spawn(lazy(move || get_opengl_task)).expect("Unable to spawn get_opengl_task");
    let opengl_rx = opengl_rx.map(|x| x);
    match opengl_rx.wait() {
        Ok(data) => {
            debug!("Got response from SendMetricsOpenGL Task: {:?}", data);
            data
        },
        Err(err) => {
            error!("Error response from SendMetricsOpenGL Task: {:?}", err);
            vec![]
        },
    }
}

/// `run` is an example use of the crate without drawing the data.
pub fn run(config: crate::config::Config) {
    let charts = config.charts.clone();
    // Create the channel that is used to communicate with the
    // background task.
    // XXX: Create a thread::spawn, get the handle and redo.
    let (tx, rx) = mpsc::channel(4_096usize);
    let _poll_tx = tx.clone();
    let mut tokio_runtime =
        tokio::runtime::Runtime::new().expect("Unable to start the tokio runtime");
    let size = SizeInfo {
        width: 100.,
        height: 100.,
        chart_width: 100.,
        chart_height: 100.,
        cell_width: 0.,
        cell_height: 0.,
        padding_x: 0.,
        padding_y: 0.,
    };
    tokio_runtime.spawn(lazy(move || {
        async_coordinator(rx, charts, size.height, size.width, size.padding_y, size.padding_x)
    }));
    // let tokio_handle = tokio_runtime.executor().clone();
    // tokio_runtime.spawn(lazy(move || {
    //    spawn_charts_intervals(config.charts.clone(), poll_tx.clone(), tokio_handle);
    //    Ok(())
    //}));
    // tokio_runtime.run().expect("Unable to run tokio tasks");
}
