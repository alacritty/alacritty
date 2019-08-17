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
    // Maybe add CloudWatch/etc
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
                        debug!("Error from {} into TimeSeries: {:?}", response.source_url, err);
                    },
                }
            }
            charts[response.chart_index].update_series_opengl_vecs(response.series_index, size);
        }
        for chart in charts {
            // Update the loaded item counters
            debug!("Searching for AsyncLoadedItems in '{}'", chart.name);
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
            charts[chart_index].opengl_vecs[data_index].clone()
        },
    ) {
        Ok(()) => {
            if chart_index > charts.len() {
                debug!(
                    "send_metrics_opengl_vecs: oneshot::message sent for {}[OutOfBounds]",
                    chart_index
                );
            } else {
                debug!(
                    "send_metrics_opengl_vecs: oneshot::message sent for {}[InsideBounds]",
                    chart_index
                );
            }
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
    debug!("get_decorations_vecs for chart_index: {}", chart_index);
    match channel.send(
        if chart_index >= charts.len() || data_index >= charts[chart_index].decorations.len() {
            vec![]
        } else {
            charts[chart_index].decorations[data_index].opengl_vertices()
        },
    ) {
        Ok(()) => {
            if chart_index > charts.len() {
                debug!(
                    "send_decorations_opengl_vecs: oneshot::message sent for {}[OutOfBounds]",
                    chart_index
                );
            } else {
                debug!(
                    "send_decorations_opengl_vecs: oneshot::message sent for {}[InsideBounds]",
                    chart_index
                );
            }
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

/// `spawn_interval_polls` creates intervals for each series requested
/// Each series will have to reply to a mspc tx with the data
pub fn spawn_interval_polls(
    item: &MetricRequest,
    tx: mpsc::Sender<AsyncChartTask>,
) -> impl Future<Item = (), Error = ()> {
    debug!("spawn_interval_polls: Starting for item={:?}", item);
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

/// `run` is an example use of the crate without drawing the data.
pub fn run(config: crate::config::Config) {
    let charts = config.charts.clone();
    let mut chart_index = 0usize;
    // Create the channel that is used to communicate with the
    // background task.
    let (tx, rx) = mpsc::channel(4_096usize);
    let poll_tx = tx.clone();
    tokio::run(lazy(move || {
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
        tokio::spawn(lazy(move || {
            async_coordinator(rx, charts, size.height, size.width, size.padding_y, size.padding_x)
        }));
        for chart in config.charts {
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
                    let poll_tx = poll_tx.clone();
                    tokio::spawn(lazy(move || spawn_interval_polls(&data_request, poll_tx)));
                }
                series_index += 1;
            }
            chart_index += 1;
        }
        Ok(())
    }));
}
