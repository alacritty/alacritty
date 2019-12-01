//! Loads prometheus metrics every now and then and displays stats
use crate::prometheus;
use crate::SizeInfo;
use crate::TimeSeriesChart;
use crate::TimeSeriesSource;
use futures::future::lazy;
use futures::sync::{mpsc, oneshot};
use log::*;
use std::thread;
use std::time::UNIX_EPOCH;
use std::time::{Duration, Instant};
use tokio::prelude::*;
use tokio::runtime::current_thread;
use tokio::timer::Interval;
//use tokio::{prelude::*, runtime::current_thread};
use tracing::{event, span, Level};

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
    IncrementInputCounter(u64, f64),
    IncrementOutputCounter(u64, f64),
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
        }
        Err(err) => {
            error!("Error response from SendLastUpdatedEpoch Task: {:?}", err);
            0u64
        }
    }
}

pub fn increment_internal_counter(
    charts: &mut Vec<TimeSeriesChart>,
    counter_type: &'static str,
    epoch: u64,
    value: f64,
    size: SizeInfo,
) {
    for chart in charts {
        for series in &mut chart.sources {
            if counter_type == "input" {
                if let TimeSeriesSource::AlacrittyInput(ref mut input) = series {
                    input.series.upsert((epoch, Some(value)));
                }
            }
            if counter_type == "output" {
                if let TimeSeriesSource::AlacrittyOutput(ref mut output) = series {
                    output.series.upsert((epoch, Some(value)));
                }
            }
            // Update the loaded item counters
            if counter_type == "async_loaded_items" {
                if let TimeSeriesSource::AsyncLoadedItems(ref mut items) = series {
                    items.series.upsert((epoch, Some(value)));
                }
            }
        }
        chart.update_all_series_opengl_vecs(size);
    }
}

/// `send_last_updated_epoch` returns the max of all the charts in an array
/// after finding the max updated epoch, it inserts it on the other series
/// so that they also progress in time.
pub fn send_last_updated_epoch(charts: &mut Vec<TimeSeriesChart>, channel: oneshot::Sender<u64>) {
    let max: u64 = charts
        .iter()
        .map(|x| x.last_updated)
        .max()
        .unwrap_or_else(|| 0u64);
    let updated_charts: usize = charts
        .iter_mut()
        .map(|x| {
            if x.last_updated < max {
                x.sources
                    .iter_mut()
                    .map(|x| x.series_mut().upsert((max, None)))
                    .sum()
            } else {
                0usize
            }
        })
        .sum();
    debug!(
        "send_last_updated_epoch: Progressed {} series to {} epoch",
        updated_charts, max
    );
    match channel.send(max) {
        Ok(()) => {
            debug!(
                "send_last_updated_epoch: oneshot::message sent with payload {}",
                charts
                    .iter()
                    .map(|x| x.last_updated)
                    .max()
                    .unwrap_or_else(|| 0u64)
            );
        }
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
    let span = span!(
        Level::DEBUG,
        "load_http_response",
        idx = response.chart_index
    );
    let _enter = span.enter();
    event!(Level::DEBUG, "load_http_response: Starting");
    if let Some(data) = response.data {
        if data.status != "success" {
            return;
        }
        let mut ok_records = 0;
        if response.chart_index < charts.len()
            && response.series_index < charts[response.chart_index].sources.len()
        {
            if let TimeSeriesSource::PrometheusTimeSeries(ref mut prom) =
                charts[response.chart_index].sources[response.series_index]
            {
                match prom.load_prometheus_response(data) {
                    Ok(num_records) => {
                        event!(Level::INFO,
                            "load_http_response:(Chart: {}, Series: {}) {} records from {} into TimeSeries",
                            response.chart_index, response.series_index, num_records, response.source_url
                        );
                        ok_records = num_records;
                    }
                    Err(err) => {
                        event!(Level::DEBUG,
                            "load_http_response:(Chart: {}, Series: {}) Error Loading {} into TimeSeries: {:?}",
                            response.chart_index, response.series_index, response.source_url, err
                        );
                    }
                }
                event!(
                    Level::DEBUG,
                    "load_http_response:(Chart: {}, Series: {}) After loading. TimeSeries is: {:?}",
                    response.chart_index,
                    response.series_index,
                    charts[response.chart_index].sources[response.series_index]
                );
            }
            charts[response.chart_index].update_series_opengl_vecs(response.series_index, size);
        }
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        increment_internal_counter(charts, "async_loaded_items", now, ok_records as f64, size);
    }
}

/// `send_metrics_opengl_vecs` is called by async_coordinator when a task of
/// type SendMetricsOpenGLData is received, it should contain the chart index
/// to represent as OpenGL vertices, it returns data through the channel parameter
pub fn send_metrics_opengl_vecs(
    charts: &[TimeSeriesChart],
    chart_index: usize,
    series_index: usize,
    channel: oneshot::Sender<Vec<f32>>,
) {
    event!(
        Level::DEBUG,
        "send_metrics_opengl_vecs:(Chart: {}, Series: {}): Request received",
        chart_index,
        series_index
    );
    match channel.send(
        if chart_index >= charts.len() || series_index >= charts[chart_index].sources.len() {
            vec![]
        } else {
            charts[chart_index].get_deduped_opengl_vecs(series_index)
        },
    ) {
        Ok(()) => {
            event!(
                Level::DEBUG,
                "send_metrics_opengl_vecs:(Chart: {}, Series: {}) oneshot::message sent",
                chart_index,
                series_index
            );
        }
        Err(err) => event!(
            Level::ERROR,
            "send_metrics_opengl_vecs:(Chart: {}, Series: {}) Error sending: {:?}",
            chart_index,
            series_index,
            err
        ),
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
    event!(
        Level::DEBUG,
        "send_decorations_vecs for chart_index: {}",
        chart_index
    );
    match channel.send(
        if chart_index >= charts.len() || data_index >= charts[chart_index].decorations.len() {
            vec![]
        } else {
            event!(
                Level::DEBUG,
                "send_decorations_opengl_vecs Sending vertices: {:?}",
                charts[chart_index].decorations[data_index].opengl_vertices()
            );
            charts[chart_index].decorations[data_index].opengl_vertices()
        },
    ) {
        Ok(()) => {
            event!(
                Level::DEBUG,
                "send_decorations_opengl_vecs: oneshot::message sent for index: {}",
                chart_index
            );
        }
        Err(err) => event!(
            Level::ERROR,
            "send_decorations_opengl_vecs: Error sending: {:?}",
            err
        ),
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
    event!(
        Level::DEBUG,
        "change_display_size for height: {}, width: {}, padding_y: {}, padding_x: {}",
        height,
        width,
        padding_y,
        padding_x
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
        Ok(()) => event!(
            Level::DEBUG,
            "change_display_size: Sent reply back to resize notifier, new size: {:?}",
            size
        ),
        Err(err) => event!(
            Level::ERROR,
            "change_display_size: Error sending: {:?}",
            err
        ),
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
    event!(
        Level::DEBUG,
        "async_coordinator: Starting, height: {}, width: {}, padding_y: {}, padding_x {}",
        height,
        width,
        padding_y,
        padding_x
    );
    for chart in &mut charts {
        // Update the loaded item counters
        event!(
            Level::DEBUG,
            "Finishing setup for sources in chart: '{}'",
            chart.name
        );
        for series in &mut chart.sources {
            series.init();
        }
    }
    let mut size = SizeInfo {
        height,
        width,
        padding_y,
        padding_x,
        ..SizeInfo::default()
    };
    rx.for_each(move |message| {
        event!(Level::DEBUG, "async_coordinator: message: {:?}", message);
        match message {
            AsyncChartTask::LoadResponse(req) => load_http_response(&mut charts, req, size),
            AsyncChartTask::SendMetricsOpenGLData(chart_index, data_index, channel) => {
                send_metrics_opengl_vecs(&charts, chart_index, data_index, channel);
            }
            AsyncChartTask::SendDecorationsOpenGLData(chart_index, data_index, channel) => {
                send_decorations_opengl_vecs(&charts, chart_index, data_index, channel);
            }
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
            }
            AsyncChartTask::IncrementInputCounter(epoch, value) => {
                increment_internal_counter(&mut charts, "input", epoch, value, size);
            }
            AsyncChartTask::IncrementOutputCounter(epoch, value) => {
                increment_internal_counter(&mut charts, "output", epoch, value, size);
            }
            AsyncChartTask::SendLastUpdatedEpoch(channel) => {
                send_last_updated_epoch(&mut charts, channel);
            }
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
    event!(
        Level::DEBUG,
        "fetch_prometheus_response:(Chart: {}, Series: {}) Starting",
        item.chart_index,
        item.series_index
    );
    let url = prometheus::PrometheusTimeSeries::prepare_url(&item.source_url, item.capacity as u64)
        .unwrap();
    let url_copy = item.source_url.clone();
    let chart_index = item.chart_index;
    let series_index = item.series_index;
    prometheus::get_from_prometheus(url.clone())
        .timeout(Duration::from_secs(item.pull_interval))
        .or_else(move |e| {
            if e.is_elapsed() {
            event!(
                Level::INFO,
                "fetch_prometheus_response:(Chart: {}, Series: {}) TimeOut accesing: {}", chart_index, series_index, url_copy);
            } else {
            event!(
                Level::INFO,
                "fetch_prometheus_response:(Chart: {}, Series: {}) err={:?}", chart_index, series_index, e);
            };
            // Instead of an error, return this so we can retry later.
            // XXX: Maybe exponential retries in the future.
            Ok(hyper::Chunk::from(
                r#"{ "status":"error","data":{"resultType":"scalar","result":[]}}"#,
            ))
        })
        .and_then(move |value| {
            event!(
                Level::DEBUG,
                "fetch_prometheus_response:(Chart: {}, Series: {}) Prometheus raw value={:?}",
                chart_index, series_index, value
            );
            let res = prometheus::parse_json(&value);
            tx.send(AsyncChartTask::LoadResponse(MetricRequest {
                source_url: item.source_url.clone(),
                chart_index: item.chart_index,
                series_index: item.series_index,
                pull_interval: item.pull_interval,
                data: res.clone(),
                capacity: item.capacity,
            }))
            .map_err(move |e| {
            event!(
                Level::ERROR,
                    "fetch_prometheus_response:(Chart: {}, Series: {}) unable to send data back to coordinator; err={:?}",
                    chart_index, series_index, e
                )
            })
            .and_then(|_| Ok(()))
        })
        .map_err(|_| {
            // This error is quite meaningless and get_from_prometheus already
            // has shown the error message that contains the actual failure.
        })
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
        let mut series_index = 0usize;
        for series in chart.sources {
            if let TimeSeriesSource::PrometheusTimeSeries(ref prom) = series {
                event!(
                    Level::DEBUG,
                    "spawn_charts_intervals:(Chart: {}, Series: {}) - Adding interval run for '{}'",
                    chart_index,
                    series_index,
                    chart.name
                );
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
                    .spawn(lazy(move || {
                        spawn_datasource_interval_polls(&data_request, charts_tx)
                    }))
                    .expect(&format!("spawn_charts_intervals:(Chart: {}, Series: {}) Error spawning datasource internal polls", chart_index, series_index));
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
    event!(
        Level::DEBUG,
        "spawn_datasource_interval_polls:(Chart: {}, Series: {}) Starting for item={:?}",
        item.chart_index,
        item.series_index,
        item
    );
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
            event!(
                Level::DEBUG,
                    "spawn_datasource_interval_polls:(Chart: {}, Series: {}) Interval triggered for {:?} at instant={:?}",
                    async_metric_item.chart_index, async_metric_item.series_index, async_metric_item.source_url, instant
                );
                fetch_prometheus_response(async_metric_item.clone(), tx.clone()).and_then(|res| {
            event!(
                Level::DEBUG,
                    "spawn_datasource_interval_polls:(Chart: {}, Series: {}) Response {:?}", async_metric_item.chart_index, async_metric_item.series_index, res);
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
        .map_err(move |e| {
            event!(
                Level::ERROR,
                "get_metric_opengl_vecs:(Chart: {}, Series: {}) Sending {} Task. err={:?}",
                chart_idx,
                series_idx,
                request_type,
                e
            )
        })
        .and_then(move |_res| {
            event!(
                Level::DEBUG,
                "get_metric_opengl_vecs:(Chart: {}, Series: {}) Sent Request for {} Task",
                chart_idx,
                series_idx,
                request_type
            );
            Ok(())
        });
    tokio_handle
        .spawn(lazy(move || get_opengl_task))
        .expect(&format!(
            "get_metric_opengl_vecs:(Chart: {}, Series: {}) Unable to spawn get_opengl_task",
            chart_idx, series_idx
        ));
    let opengl_rx = opengl_rx.map(|x| x);
    match opengl_rx.wait() {
        Ok(data) => {
            event!(
                Level::DEBUG,
                "get_metric_opengl_vecs:(Chart: {}, Series: {}) Response from {} Task: {:?}",
                chart_idx,
                series_idx,
                request_type,
                data
            );
            data
        }
        Err(err) => {
            event!(
                Level::ERROR,
                "get_metric_opengl_vecs:(Chart: {}, Series: {}) Error from {} Task: {:?}",
                chart_idx,
                series_idx,
                request_type,
                err
            );
            vec![]
        }
    }
}

/// `tokio_default_setup` creates a default channels and handles, this should be used mostly for testing
/// to avoid having to create all the tokio boilerplate, I would like to return a struct but
/// the ownership and cloning and moving of the separate parts does not seem possible then
pub fn tokio_default_setup() -> (
    //std::sync::mpsc::Receiver<current_thread::Handle>,
    //thread::JoinHandle<()>,
    current_thread::Handle,
    mpsc::Sender<AsyncChartTask>,
    oneshot::Sender<()>,
) {
    // Create the channel that is used to communicate with the
    // charts background task.
    let (charts_tx, charts_rx) = mpsc::channel(4_096usize);
    // Create a channel to receive a handle from Tokio
    //
    let (handle_tx, handle_rx) = std::sync::mpsc::channel();
    // Start the Async I/O runtime, this needs to run in a background thread because in OSX,
    // only the main thread can write to the graphics card.
    let (_tokio_thread, tokio_shutdown) = spawn_async_tasks(
        vec![],
        charts_tx.clone(),
        charts_rx,
        handle_tx,
        SizeInfo::default(),
    );
    let tokio_handle = handle_rx
        .recv()
        .expect("Unable to get the tokio handle in a background thread");

    (
        //handle_rx,
        //tokio_thread,
        tokio_handle,
        charts_tx,
        tokio_shutdown,
    )
}

/// `spawn_async_tasks` Starts a background thread to be used for tokio for async tasks
pub fn spawn_async_tasks(
    charts: Vec<TimeSeriesChart>,
    charts_tx: mpsc::Sender<AsyncChartTask>,
    charts_rx: mpsc::Receiver<AsyncChartTask>,
    handle_tx: std::sync::mpsc::Sender<current_thread::Handle>,
    charts_size_info: SizeInfo,
) -> (thread::JoinHandle<()>, oneshot::Sender<()>) {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let tokio_thread = ::std::thread::Builder::new()
        .name("async I/O".to_owned())
        .spawn(move || {
            let mut tokio_runtime =
                current_thread::Runtime::new().expect("Failed to start new tokio Runtime");
            info!("Tokio runtime created.");

            // Give a handle to the runtime back to the main thread.
            handle_tx
                .send(tokio_runtime.handle())
                .expect("Unable to give runtime handle to the main thread");
            let async_charts = charts.clone();
            tokio_runtime.spawn(lazy(move || {
                async_coordinator(
                    charts_rx,
                    async_charts,
                    charts_size_info.height,
                    charts_size_info.width,
                    charts_size_info.padding_y,
                    charts_size_info.padding_x,
                )
            }));
            let tokio_handle = tokio_runtime.handle().clone();
            tokio_runtime.spawn(lazy(move || {
                spawn_charts_intervals(charts.clone(), charts_tx, tokio_handle);
                Ok(())
            }));
            tokio_runtime.spawn({
                shutdown_rx
                    .map(|_x| info!("Got shutdown signal for Tokio"))
                    .map_err(|err| error!("Error on the tokio shutdown channel: {:?}", err))
            });
            tokio_runtime.run().expect("Unable to run Tokio tasks");
            info!("Tokio runtime finished.");
        })
        .expect("Unable to start async I/O thread");
    (tokio_thread, shutdown_tx)
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
        async_coordinator(
            rx,
            charts,
            size.height,
            size.width,
            size.padding_y,
            size.padding_x,
        )
    }));
    // let tokio_handle = tokio_runtime.executor().clone();
    // tokio_runtime.spawn(lazy(move || {
    //    spawn_charts_intervals(config.charts.clone(), poll_tx.clone(), tokio_handle);
    //    Ok(())
    //}));
    // tokio_runtime.run().expect("Unable to run tokio tasks");
}
