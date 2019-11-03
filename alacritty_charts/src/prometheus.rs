use crate::Rgb;
use crate::ValueCollisionPolicy;
/// `Prometheus HTTP API` data structures
use hyper::rt::{Future, Stream};
use hyper::Client;
use hyper_tls::HttpsConnector;
use log::*;
use percent_encoding::{utf8_percent_encode, DEFAULT_ENCODE_SET};
use std::collections::HashMap;
use std::time::UNIX_EPOCH;
// The below data structures for parsing something like:
//  {
//   "data": {
//     "result": [
//       {
//         "metric": {
//           "__name__": "up",
//           "instance": "localhost:9090",
//           "job": "prometheus"
//         },
//         "value": [
//           1557052757.816,
//           "1"
//         ]
//       },{...}
//     ],
//     "resultType": "vector"
//   },
//   "status": "success"
// }
/// `HTTPMatrixResult` contains Range Vectors, data is stored like this
/// [[Epoch1, Metric1], [Epoch2, Metric2], ...]
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone)]
pub struct HTTPMatrixResult {
    #[serde(rename = "metric")]
    pub labels: HashMap<String, String>,
    pub values: Vec<Vec<serde_json::Value>>,
}

/// `HTTPVectorResult` contains Instant Vectors, data is stored like this
/// [Epoch1, Metric1, Epoch2, Metric2, ...]
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone)]
pub struct HTTPVectorResult {
    #[serde(rename = "metric")]
    pub labels: HashMap<String, String>,
    pub value: Vec<serde_json::Value>,
}

/// `HTTPResponseData` may be one of these types:
/// https://prometheus.io/docs/prometheus/latest/querying/api/#expression-query-result-formats
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(tag = "resultType")]
pub enum HTTPResponseData {
    #[serde(rename = "vector")]
    Vector { result: Vec<HTTPVectorResult> },
    #[serde(rename = "matrix")]
    Matrix { result: Vec<HTTPMatrixResult> },
    #[serde(rename = "scalar")]
    Scalar { result: Vec<serde_json::Value> },
    #[serde(rename = "string")]
    String { result: Vec<serde_json::Value> },
}

impl Default for HTTPResponseData {
    fn default() -> HTTPResponseData {
        HTTPResponseData::Vector {
            result: vec![HTTPVectorResult::default()],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone)]
pub struct HTTPResponse {
    pub data: HTTPResponseData,
    pub status: String,
}

/// Transforms an serde_json::Value into an optional u64
/// The epoch coming from is a float (epoch with millisecond),
/// but our internal representation is u64
pub fn prometheus_epoch_to_u64(input: &serde_json::Value) -> Option<u64> {
    if input.is_number() {
        if let Some(input) = input.as_f64() {
            return Some(input as u64);
        }
    }
    None
}

/// Transforms an serde_json::Value into an optional f64
pub fn serde_json_to_num(input: &serde_json::Value) -> Option<f64> {
    if input.is_string() {
        if let Some(input) = input.as_str() {
            if let Ok(value) = input.parse() {
                return Some(value);
            }
        }
    }
    None
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrometheusTimeSeries {
    /// The Name of this TimesSeries
    #[serde(default)]
    pub name: String,

    /// The TimeSeries metrics storage
    #[serde(default)]
    pub series: crate::TimeSeries,

    /// The TimeSeries metrics storage
    #[serde(default)]
    pub data: HTTPResponseData,

    /// The URL were Prometheus metrics may be acquaired
    #[serde(default)]
    pub source: String,

    /// The URL were Prometheus metrics may be acquaired
    #[serde(skip)]
    pub url: hyper::Uri,

    /// A response may be vector, matrix, scalar or string
    #[serde(default)]
    pub data_type: String,

    /// The Labels key and value, if any, to match the response
    #[serde(default)]
    #[serde(rename = "labels")]
    pub required_labels: HashMap<String, String>,

    /// The time in secondso to get the metrics from Prometheus
    /// Shouldn't be faster than the scrape interval for the Target
    #[serde(default)]
    #[serde(rename = "refresh")]
    pub pull_interval: usize,

    /// The color of the TimeSeries
    #[serde(default)]
    pub color: Rgb,

    /// The transparency of the TimeSeries
    #[serde(default)]
    pub alpha: f32,
}

impl Default for PrometheusTimeSeries {
    fn default() -> PrometheusTimeSeries {
        PrometheusTimeSeries {
            name: String::from("Unset"),
            series: crate::TimeSeries {
                collision_policy: ValueCollisionPolicy::Overwrite,
                ..crate::TimeSeries::default()
            },
            data: HTTPResponseData::default(),
            source: String::from(""),
            url: hyper::Uri::default(),
            pull_interval: 15,
            data_type: String::from("vector"),
            required_labels: HashMap::new(),
            color: crate::Rgb::default(),
            alpha: 1.0,
        }
    }
}
impl PrometheusTimeSeries {
    /// `new` returns a new PrometheusTimeSeries. it takes a URL where to load
    /// the data from and a pull_interval, this should match scrape interval in
    /// Prometheus Server side to avoid pulling the same values over and over.
    pub fn new(
        url_param: String,
        pull_interval: usize,
        data_type: String,
        required_labels: HashMap<String, String>,
    ) -> Result<PrometheusTimeSeries, String> {
        let mut res = PrometheusTimeSeries {
            name: String::from("Unset"),
            series: crate::TimeSeries {
                collision_policy: ValueCollisionPolicy::Overwrite,
                ..crate::TimeSeries::default()
            },
            data: HTTPResponseData::default(),
            source: url_param,
            url: hyper::Uri::default(),
            pull_interval,
            data_type,
            required_labels,
            ..PrometheusTimeSeries::default()
        };
        match PrometheusTimeSeries::prepare_url(&res.source, res.series.metrics_capacity as u64) {
            Ok(url) => {
                res.url = url;
                Ok(res)
            }
            Err(err) => Err(err),
        }
    }

    /// `init` sets up several properties that would be too complicated to setup via yaml config
    pub fn init(&mut self) {
        self.series.collision_policy = ValueCollisionPolicy::Overwrite;
    }

    /// `prepare_url` loads self.source into a hyper::Uri
    /// It also adds a epoch-start and epoch-end to the
    /// URL depending on the metrics capacity
    pub fn prepare_url(source: &str, metrics_capacity: u64) -> Result<hyper::Uri, String> {
        // url should be like ("http://localhost:9090/api/v1/query?{}",query)
        // We split self.source into url_base_path?params
        // XXX: We only support one param, if more params are added with &
        //      they are percent encoded.
        // But sounds like configuration would become easy to mess up.
        let url_parts: Vec<&str> = source.split('?').collect();
        if url_parts.len() < 2 {
            return Err(String::from(
                "Unable to get url_parts, expected http://host:port/location?params",
            ));
        }
        let url_base_path = url_parts[0];
        // XXX: We only support one input param
        let url_param = url_parts[1..].join("");
        let encoded_url_param = utf8_percent_encode(&url_param, DEFAULT_ENCODE_SET).to_string();
        let mut encoded_url = format!("{}?{}", url_base_path, encoded_url_param);
        // If this is a query_range, we need to add time range
        if encoded_url.contains("/api/v1/query_range?") {
            let end = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let start = end - metrics_capacity;
            let step = "1"; // Maybe we can change granularity later
            encoded_url = format!("{}&start={}&end={}&step={}", encoded_url, start, end, step);
        }
        match encoded_url.parse::<hyper::Uri>() {
            Ok(url) => {
                if url.scheme_part() == Some(&hyper::http::uri::Scheme::HTTP)
                    || url.scheme_part() == Some(&hyper::http::uri::Scheme::HTTPS)
                {
                    debug!("Setting url to: {:?}", url);
                    Ok(url)
                } else {
                    error!("Only HTTP and HTTPS protocols are supported");
                    Err(format!("Unsupported protocol: {:?}", url.scheme_part()))
                }
            }
            Err(err) => {
                error!("Unable to parse url: {}", err);
                Err(format!("Unable to parse URL: {:?}", err))
            }
        }
    }

    /// `match_metric_labels` checks the labels in the incoming
    /// PrometheusData contains the required labels
    pub fn match_metric_labels(&self, metric_labels: &HashMap<String, String>) -> bool {
        for (required_label, required_value) in &self.required_labels {
            match metric_labels.get(required_label) {
                Some(return_value) => {
                    if return_value != required_value {
                        debug!(
                            "Skip: Required label '{}' exists but required value: '{}' does not \
                             match current value: '{}'",
                            required_label, required_value, return_value
                        );
                        return false;
                    } else {
                        debug!(
                            "Good: Required label '{}' exists and matches required value",
                            required_label
                        );
                    }
                }
                None => {
                    debug!("Skip: Required label '{}' does not exists", required_label);
                    return false;
                }
            }
        }
        true
    }

    /// `load_prometheus_response` loads data from PrometheusResponse into
    /// the internal `series`, returns the number of items or an error
    /// string
    pub fn load_prometheus_response(&mut self, res: HTTPResponse) -> Result<usize, String> {
        let mut loaded_items = 0;
        if res.status != "success" {
            return Ok(0usize);
        }
        debug!(
            "load_prometheus_response: before upsert, series is: {:?}",
            self.series
        );
        debug!("load_prometheus_response: Checking data: {:?}", res.data);
        match res.data {
            HTTPResponseData::Vector { result: results } => {
                // labeled metrics returned as a 2 items vector:
                // [ {metric: {l: X}, value: [epoch1,sample1]}
                //   {metric: {l: Y}, value: [epoch2,sample2]} ]
                for metric_data in results.iter() {
                    if self.match_metric_labels(&metric_data.labels) {
                        // The result array is  [epoch, value, epoch, value]
                        if metric_data.value.len() == 2 {
                            let opt_epoch = prometheus_epoch_to_u64(&metric_data.value[0]);
                            let value = serde_json_to_num(&metric_data.value[1]);
                            if let Some(epoch) = opt_epoch {
                                loaded_items += self.series.upsert((epoch, value));
                            }
                        }
                    }
                }
            }
            HTTPResponseData::Matrix { result: results } => {
                // labeled metrics returned as a matrix:
                // [ {metric: {l: X}, value: [[epoch1,sample2],[...]]}
                //   {metric: {l: Y}, value: [[epoch3,sample4],[...]]} ]
                for metric_data in results.iter() {
                    if self.match_metric_labels(&metric_data.labels) {
                        // The result array is  [epoch, value, epoch, value]
                        for item_value in &metric_data.values {
                            for item in item_value.chunks_exact(2) {
                                let opt_epoch = prometheus_epoch_to_u64(&item[0]);
                                let value = serde_json_to_num(&item[1]);
                                if let Some(epoch) = opt_epoch {
                                    debug!(
                                        "load_prometheus_response: Upserting from Matrix({},{:?}),",
                                        epoch, value
                                    );
                                    loaded_items += self.series.upsert((epoch, value));
                                }
                            }
                        }
                    }
                }
            }
            HTTPResponseData::Scalar { result } | HTTPResponseData::String { result } => {
                // unlabeled metrics returned as a 2 items vector
                // [epoch1,sample2]
                // XXX: no example found for String.
                if result.len() > 1 {
                    let opt_epoch = prometheus_epoch_to_u64(&result[0]);
                    let value = serde_json_to_num(&result[1]);
                    if let Some(epoch) = opt_epoch {
                        loaded_items += self.series.upsert((epoch, value));
                    }
                }
            }
        };
        if loaded_items > 0 {
            self.series.calculate_stats();
        }
        debug!(
            "load_prometheus_response: after upsert, series is: {:?}",
            self.series
        );
        Ok(loaded_items)
    }
}

/// `get_from_prometheus` is an async operation that returns an Optional
/// PrometheusResponse
pub fn get_from_prometheus(
    url: hyper::Uri,
) -> impl Future<Item = hyper::Chunk, Error = hyper::Uri> {
    info!("get_from_prometheus: Loading Prometheus URL: {}", url);
    let request = if url.scheme_part() == Some(&hyper::http::uri::Scheme::HTTP) {
        Client::new().get(url.clone())
    } else {
        // 4 is number of blocking DNS threads
        let https = HttpsConnector::new(4).unwrap();
        Client::builder()
            .build::<_, hyper::Body>(https)
            .get(url.clone())
    };
    let url_copy = url.clone();
    request
        .and_then(|res| {
            res.into_body()
                // A hyper::Body is a Stream of Chunk values. We need a
                // non-blocking way to get all the chunks so we can deserialize the response.
                // The concat2() function takes the separate body chunks and makes one
                // hyper::Chunk value with the contents of the entire body
                .concat2()
                .and_then(|body| {
                    debug!("get_from_prometheus: Body={:?}", body);
                    Ok(body)
                })
        })
        .map_err(|err| {
            error!("get_from_prometheus: Error loading '{:?}'", err);
            url_copy
        })
}
/// `parse_json` transforms a hyper body chunk into a possible
/// PrometheusResponse, mostly used for testing
pub fn parse_json(body: &hyper::Chunk) -> Option<HTTPResponse> {
    let prom_res: Result<HTTPResponse, serde_json::Error> = serde_json::from_slice(&body);
    // XXX: Figure out how to return the error
    match prom_res {
        Ok(v) => {
            debug!("parse_json: returned JSON={:?}", v);
            Some(v)
        }
        Err(err) => {
            error!("Unable to parse JSON err={:?}", err);
            None
        }
    }
}
/// XXX: REMOVE
/// Implement PartialEq for PrometheusTimeSeries because the field
/// tokio_core should be ignored
impl PartialEq<PrometheusTimeSeries> for PrometheusTimeSeries {
    fn eq(&self, other: &PrometheusTimeSeries) -> bool {
        self.series == other.series
            && self.url == other.url
            && self.pull_interval == other.pull_interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prometheus::HTTPResponseData::Vector;
    use crate::MissingValuesPolicy;
    use crate::TimeSeries;
    use crate::TimeSeriesStats;
    use tokio_core::reactor::Core;
    fn init_log() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn it_skips_prometheus_errors() {
        // This URL has the end time BEFORE the start time
        let test0_res: Result<PrometheusTimeSeries, String> = PrometheusTimeSeries::new(
            String::from("http://localhost:9090/api/v1/query_range?query=node_load1&start=1558253499&end=1558253479&step=1"),
            15,
            String::from("matrix"),
            HashMap::new(),
        );
        assert_eq!(test0_res.is_ok(), true);
        // A json returned by prometheus
        let test0_json = hyper::Chunk::from(
            r#"
            {
              "status": "error",
              "errorType": "bad_data",
              "error": "end timestamp must not be before start time"
            }
            "#,
        );
        let res0_json = parse_json(&test0_json);
        assert_eq!(res0_json.is_none(), true);
    }

    #[test]
    fn it_loads_prometheus_scalars() {
        let test0_res: Result<PrometheusTimeSeries, String> = PrometheusTimeSeries::new(
            String::from("http://localhost:9090/api/v1/query?query=1"),
            15,
            String::from("scalar"),
            HashMap::new(),
        );
        assert_eq!(test0_res.is_ok(), true);
        let mut test0 = test0_res.unwrap();
        // A json returned by prometheus
        let test0_json = hyper::Chunk::from(
            r#"
            { "status":"success",
              "data":{
                "resultType":"scalar",
                "result":[1558283674.829,"1"]
              }
            }"#,
        );
        let res0_json = parse_json(&test0_json);
        assert_eq!(res0_json.is_some(), true);
        let res0_load = test0.load_prometheus_response(res0_json.unwrap());
        // 1 items should have been loaded
        assert_eq!(res0_load, Ok(1usize));
        // This json is missing the value after the epoch
        let test1_json = hyper::Chunk::from(
            r#"
            { "status":"success",
              "data":{
                "resultType":"scalar",
                "result":[1558283674.829]
              }
            }"#,
        );
        let res1_json = parse_json(&test1_json);
        assert_eq!(res1_json.is_some(), true);
        let res1_load = test0.load_prometheus_response(res1_json.unwrap());
        // 0 items should have been loaded, because there's no value
        assert_eq!(res1_load, Ok(0usize));
    }

    #[test]
    fn it_loads_prometheus_matrix() {
        init_log();
        let test0_res: Result<PrometheusTimeSeries, String> = PrometheusTimeSeries::new(
            String::from("http://localhost:9090/api/v1/query_range?query=node_load1&start=1558253469&end=1558253479&step=1"),
            15,
            String::from("matrix"),
            HashMap::new()
        );
        assert_eq!(test0_res.is_ok(), true);
        let mut test0 = test0_res.unwrap();
        // Let's create space for 15, but we will receive 11 records:
        test0.series = test0.series.with_capacity(15usize);
        // A json returned by prometheus
        let test0_json = hyper::Chunk::from(
            r#"
            {
              "status": "success",
              "data": {
                "resultType": "matrix",
                "result": [
                  {
                    "metric": {
                      "__name__": "node_load1",
                      "instance": "localhost:9100",
                      "job": "node_exporter"
                    },
                    "values": [
                        [1558253469,"1.69"],[1558253470,"1.70"],[1558253471,"1.71"],
                        [1558253472,"1.72"],[1558253473,"1.73"],[1558253474,"1.74"],
                        [1558253475,"1.75"],[1558253476,"1.76"],[1558253477,"1.77"],
                        [1558253478,"1.78"],[1558253479,"1.79"]]
                  }
                ]
              }
            }"#,
        );
        let res0_json = parse_json(&test0_json);
        assert_eq!(res0_json.is_some(), true);
        let res0_load = test0.load_prometheus_response(res0_json.clone().unwrap());
        // 11 items should have been loaded in the node_exporter
        assert_eq!(res0_load, Ok(11usize));
        debug!(
            "it_loads_prometheus_matrix NOTVEC: {:?}",
            test0.series.metrics
        );
        let loaded_data = test0.series.as_vec();
        debug!("it_loads_prometheus_matrix Data: {:?}", loaded_data);
        assert_eq!(loaded_data[0], (1558253469, Some(1.69f64)));
        assert_eq!(loaded_data[1], (1558253470, Some(1.70f64)));
        assert_eq!(loaded_data[5], (1558253474, Some(1.74f64)));
        // Let's add one more item and subtract one item from the array
        let test1_json = hyper::Chunk::from(
            r#"
            {
              "status": "success",
              "data": {
                "resultType": "matrix",
                "result": [
                  {
                    "metric": {
                      "__name__": "node_load1",
                      "instance": "localhost:9100",
                      "job": "node_exporter"
                    },
                    "values": [
                        [1558253471,"1.71"],[1558253472,"1.72"],[1558253473,"1.73"],
                        [1558253474,"1.74"],[1558253475,"1.75"],[1558253476,"1.76"],
                        [1558253477,"1.77"],[1558253478,"1.78"],[1558253479,"1.79"],
                        [1558253480,"1.80"],[1558253481,"1.81"],[1558253482,"1.82"],
                        [1558253483,"1.83"],[1558253484,"1.84"],[1558253485,"1.85"],
                        [1558253486,"1.86"]]
                  }
                ]
              }
            }"#,
        );
        let res1_json = parse_json(&test1_json);
        assert_eq!(res1_json.is_some(), true);
        debug!(
            "it_loads_prometheus_matrix NOTVEC: {:?}",
            test0.series.metrics
        );
        let loaded_data = test0.series.as_vec();
        debug!("it_loads_prometheus_matrix Data: {:?}", loaded_data);
        let res1_load = test0.load_prometheus_response(res1_json.clone().unwrap());
        // 7 items should have been loaded in the node_exporter, 9 already existed
        // 2 should have been rotated
        assert_eq!(res1_load, Ok(7usize));

        // Let's test reloading the data:
        let res1_load = test0.load_prometheus_response(res1_json.clone().unwrap());
        // Now 0 records should have been loaded:
        assert_eq!(res1_load, Ok(0usize));
        debug!(
            "it_loads_prometheus_matrix NOTVEC: {:?}",
            test0.series.metrics
        );
        let loaded_data = test0.series.metrics.clone();
        debug!("it_loads_prometheus_matrix Data: {:?}", loaded_data);
        assert_eq!(loaded_data[0], (1558253484, Some(1.84f64)));
        assert_eq!(loaded_data[3], (1558253472, Some(1.72f64)));
        assert_eq!(loaded_data[5], (1558253474, Some(1.74f64)));
        // This json is missing the value after the epoch
        let test2_json = hyper::Chunk::from(
            r#"
            {
              "status": "success",
              "data": {
                "resultType": "matrix",
                "result": [
                  {
                    "metric": {
                      "__name__": "node_load1",
                      "instance": "localhost:9100",
                      "job": "node_exporter"
                    },
                    "values": [
                        [1558253478]
                    ]
                  }
                ]
              }
            }"#,
        );
        let res2_json = parse_json(&test2_json);
        assert_eq!(res2_json.is_some(), true);
        let res2_load = test0.load_prometheus_response(res2_json.unwrap());
        // 0 items should have been loaded, missing metric after epoch.
        assert_eq!(res2_load, Ok(0usize));
    }

    #[test]
    fn it_calculates_stats() {
        let metric_labels = HashMap::new();
        let test0_res: Result<PrometheusTimeSeries, String> = PrometheusTimeSeries::new(
            String::from("http://localhost:9090/api/v1/query?query=up"),
            15,
            String::from("vector"),
            metric_labels.clone(),
        );
        assert_eq!(test0_res.is_ok(), true);
        let mut test0 = test0_res.unwrap();
        let test1_json = hyper::Chunk::from(
            r#"
            {
              "status": "success",
              "data": {
                "resultType": "matrix",
                "result": [
                  {
                    "metric": {
                      "__name__": "node_load1",
                      "instance": "localhost:9100",
                      "job": "node_exporter"
                    },
                    "values": [
                      [1566918913,"4.5"],
                      [1566918914,"4.5"],
                      [1566918915,"4.5"],
                      [1566918916,"4.5"],
                      [1566918917,"4.5"],
                      [1566918918,"4.5"],
                      [1566918919,"4.25"],
                      [1566918920,"4.25"],
                      [1566918921,"4.25"],
                      [1566918922,"4.25"],
                      [1566918923,"4.25"],
                      [1566918924,"4.25"],
                      [1566918925,"4"],
                      [1566918926,"4"],
                      [1566918927,"4"],
                      [1566918928,"4"],
                      [1566918929,"4"],
                      [1566918930,"4"],
                      [1566918931,"4.75"],
                      [1566918932,"4.75"],
                      [1566918933,"4.75"],
                      [1566918934,"4.75"],
                      [1566918935,"4.75"],
                      [1566918936,"4.75"]
                    ]
                  }
                ]
              }
            }"#,
        );
        let res1_json = parse_json(&test1_json);
        assert_eq!(res1_json.is_some(), true);
        let res1_load = test0.load_prometheus_response(res1_json.unwrap());
        // 1 items should have been loaded
        assert_eq!(res1_load, Ok(24usize));
        assert_eq!(
            test0.series.as_vec(),
            vec![
                (1566918913, Some(4.5)),
                (1566918914, Some(4.5)),
                (1566918915, Some(4.5)),
                (1566918916, Some(4.5)),
                (1566918917, Some(4.5)),
                (1566918918, Some(4.5)),
                (1566918919, Some(4.25)),
                (1566918920, Some(4.25)),
                (1566918921, Some(4.25)),
                (1566918922, Some(4.25)),
                (1566918923, Some(4.25)),
                (1566918924, Some(4.25)),
                (1566918925, Some(4.)),
                (1566918926, Some(4.)),
                (1566918927, Some(4.)),
                (1566918928, Some(4.)),
                (1566918929, Some(4.)),
                (1566918930, Some(4.)),
                (1566918931, Some(4.75)),
                (1566918932, Some(4.75)),
                (1566918933, Some(4.75)),
                (1566918934, Some(4.75)),
                (1566918935, Some(4.75)),
                (1566918936, Some(4.75))
            ]
        );
        test0.series.calculate_stats();
        let test0_sum = 4.5 * 6. + 4.25 * 6. + 4. * 6. + 4.75 * 6.;
        assert_eq!(
            test0.series.stats,
            crate::TimeSeriesStats {
                first: 4.5,
                last: 4.75,
                count: 24,
                is_dirty: false,
                max: 4.75,
                min: 4.,
                sum: test0_sum,
                avg: test0_sum / 24.,
            }
        );
    }

    #[test]
    fn it_loads_prometheus_vector() {
        init_log();
        let mut metric_labels = HashMap::new();
        let test0_res: Result<PrometheusTimeSeries, String> = PrometheusTimeSeries::new(
            String::from("http://localhost:9090/api/v1/query?query=up"),
            15,
            String::from("vector"),
            metric_labels.clone(),
        );
        assert_eq!(test0_res.is_ok(), true);
        let mut test0 = test0_res.unwrap();
        // A json returned by prometheus
        let test0_json = hyper::Chunk::from(
            r#"
            {
              "status": "success",
              "data": {
                "resultType": "vector",
                "result": [
                  {
                    "metric": {
                      "__name__": "up",
                      "instance": "localhost:9090",
                      "job": "prometheus"
                    },
                    "value": [
                      1557571137.732,
                      "1"
                    ]
                  },
                  {
                    "metric": {
                      "__name__": "up",
                      "instance": "localhost:9100",
                      "job": "node_exporter"
                    },
                    "value": [
                      1557571138.732,
                      "1"
                    ]
                  }
                ]
              }
            }"#,
        );
        let res0_json = parse_json(&test0_json);
        assert_eq!(res0_json.is_some(), true);
        let res0_load = test0.load_prometheus_response(res0_json.clone().unwrap());
        // 2 items should have been loaded, one for Prometheus Server and the
        // other for Prometheus Node Exporter
        assert_eq!(res0_load, Ok(2usize));
        assert_eq!(
            test0.series.as_vec(),
            vec![(1557571137u64, Some(1.)), (1557571138u64, Some(1.))]
        );

        let test1_json = hyper::Chunk::from(
            r#"
            {
              "status": "success",
              "data": {
                "resultType": "vector",
                "result": [
                  {
                    "metric": {
                      "__name__": "up",
                      "instance": "localhost:9090",
                      "job": "prometheus"
                    },
                    "value": [
                      1557571139.732,
                      "1"
                    ]
                  },
                  {
                    "metric": {
                      "__name__": "up",
                      "instance": "localhost:9100",
                      "job": "node_exporter"
                    },
                    "value": [
                      1557571140.732,
                      "1"
                    ]
                  }
                ]
              }
            }"#,
        );
        let res1_json = parse_json(&test1_json);
        assert_eq!(res1_json.is_some(), true);

        // Make the labels match only one instance
        metric_labels.insert(String::from("job"), String::from("prometheus"));
        metric_labels.insert(String::from("instance"), String::from("localhost:9090"));
        test0.required_labels = metric_labels.clone();
        let res1_load = test0.load_prometheus_response(res1_json.clone().unwrap());
        // Only the prometheus: localhost:9090 should have been loaded with epoch 1557571139
        assert_eq!(res1_load, Ok(1usize));
        assert_eq!(
            test0.series.as_vec(),
            vec![
                (1557571137u64, Some(1.)),
                (1557571138u64, Some(1.)),
                (1557571139u64, Some(1.))
            ]
        );

        let test2_json = hyper::Chunk::from(
            r#"
            {
              "status": "success",
              "data": {
                "resultType": "vector",
                "result": [
                  {
                    "metric": {
                      "__name__": "up",
                      "instance": "localhost:9090",
                      "job": "prometheus"
                    },
                    "value": [
                      1557571141.732,
                      "1"
                    ]
                  },
                  {
                    "metric": {
                      "__name__": "up",
                      "instance": "localhost:9100",
                      "job": "node_exporter"
                    },
                    "value": [
                      1557571142.732,
                      "1"
                    ]
                  }
                ]
              }
            }"#,
        );
        let res2_json = parse_json(&test2_json);
        assert_eq!(res2_json.is_some(), true);
        // Make the labels not match
        metric_labels.insert(String::from("__name__"), String::from("down"));
        test0.required_labels = metric_labels.clone();
        let res2_load = test0.load_prometheus_response(res2_json.clone().unwrap());
        assert_eq!(res2_load, Ok(0usize));
        assert_eq!(
            test0.series.as_vec(),
            vec![
                (1557571137u64, Some(1.)),
                (1557571138u64, Some(1.)),
                (1557571139u64, Some(1.))
            ]
        );
        // This json is missing the value after the epoch
        let test3_json = hyper::Chunk::from(
            r#"
            {
              "status": "success",
              "data": {
                "resultType": "vector",
                "result": [
                  {
                    "metric": {
                      "__name__": "node_load1",
                      "instance": "localhost:9100",
                      "job": "node_exporter"
                    },
                    "value": [
                        1558253478
                    ]
                  }
                ]
              }
            }"#,
        );
        let res3_json = parse_json(&test3_json);
        assert_eq!(res3_json.is_some(), true);
        let res3_load = test0.load_prometheus_response(res3_json.unwrap());
        // 0 items should have been loaded, the data is invalid
        assert_eq!(res3_load, Ok(0usize));
    }

    #[test]
    #[ignore]
    fn it_gets_prometheus_metrics() {
        // These tests have been mocked above, but testing the actual communication
        // without creating a temporary web server is done needs this for now.
        init_log();
        // Create a Tokio Core to use for testing
        let mut core = Core::new().unwrap();
        let mut test_labels = HashMap::new();
        test_labels.insert(String::from("name"), String::from("up"));
        test_labels.insert(String::from("job"), String::from("prometheus"));
        test_labels.insert(String::from("instance"), String::from("localhost:9090"));
        // Test non plain http error:
        let test0_res: Result<PrometheusTimeSeries, String> = PrometheusTimeSeries::new(
            String::from("https://localhost:9090/api/v1/query?query=up"),
            15,
            String::from("vector"),
            test_labels.clone(),
        );
        assert_ne!(
            test0_res,
            Err(String::from("Unsupported protocol: Some(\"https\")"))
        );
        let test1_res: Result<PrometheusTimeSeries, String> = PrometheusTimeSeries::new(
            String::from("http://localhost:9090/api/v1/query?query=up"),
            15,
            String::from("vector"),
            test_labels.clone(),
        );
        assert_eq!(test1_res.is_ok(), true);
        let test1 = test1_res.unwrap();
        let res1_get = core.run(get_from_prometheus(test1.url.clone()));
        println!("get_from_prometheus: {:?}", res1_get);
        assert_eq!(res1_get.is_ok(), true);
        if let Some(prom_response) = parse_json(&res1_get.unwrap()) {
            // This requires a Prometheus Server running locally
            // XXX: mock this.
            // Example playload:
            // {"status":"success","data":{"resultType":"vector","result":[
            //   {"metric":{"__name__":"up","instance":"localhost:9090","job":"prometheus"},
            //    "value":[1558270835.417,"1"]},
            //   {"metric":{"__name__":"up","instance":"localhost:9100","job":"node_exporter"},
            //    "value":[1558270835.417,"1"]}
            // ]}}
            assert_eq!(prom_response.status, String::from("success"));
            let mut found_prometheus_job_metric = false;
            if let HTTPResponseData::Vector { result: results } = prom_response.data {
                for prom_item in results.iter() {
                    if test1.match_metric_labels(&test_labels) {
                        assert_eq!(prom_item.value.len(), 2);
                        assert_eq!(prom_item.value[1], String::from("1"));
                        found_prometheus_job_metric = true;
                    }
                }
            }
            assert_eq!(found_prometheus_job_metric, true);
        }
    }

    #[test]
    fn it_does_not_duplicate_epochs() {
        init_log();
        let test_labels = HashMap::new();
        let mut test = PrometheusTimeSeries {
            name: String::from("load average 1 min"),
            series: TimeSeries {
                metrics: vec![
                    (1571511822, Some(1.8359375)),
                    (1571511823, Some(1.8359375)),
                    (1571511824, Some(1.8359375)),
                    (1571511825, Some(1.8359375)),
                    (1571511826, Some(1.8359375)),
                ],
                metrics_capacity: 30,
                stats: TimeSeriesStats {
                    max: 17179869184.0,
                    min: 17179869184.0,
                    avg: 17179869184.0,
                    first: 17179869184.0,
                    last: 17179869184.0,
                    count: 5,
                    sum: 1202590842880.0,
                    is_dirty: false,
                },
                collision_policy: ValueCollisionPolicy::Overwrite,
                missing_values_policy: MissingValuesPolicy::Zero,
                first_idx: 0,
                active_items: 5,
            },
            data: Vector {
                result: vec![HTTPVectorResult {
                    labels: test_labels.clone(),
                    value: vec![],
                }],
            },
            source: String::from(
                "http://localhost:9090/api/v1/query_range?query=node_memory_bytes_total",
            ),
            url: "/".parse::<hyper::Uri>().unwrap(),
            data_type: String::from(""),
            required_labels: test_labels.clone(),
            pull_interval: 15,
            color: Rgb {
                r: 207,
                g: 102,
                b: 121,
            },
            alpha: 1.0,
        };
        // This should result in adding 15 more items
        let test1_json = hyper::Chunk::from(
            r#"{
              "status":"success",
              "data":{
                "resultType":"matrix",
                "result":[{
                  "metric":{
                    "__name__":"node_load1",
                    "instance":"localhost:9100",
                    "job":"node_exporter"
                  },
                  "values":[
                    [1571511822,"1.8359322"],
                    [1571511823,"1.8359323"],
                    [1571511824,"1.8359324"],
                    [1571511825,"1.8359325"],
                    [1571511826,"1.8359326"],
                    [1571511827,"1.8359327"],
                    [1571511828,"1.8359328"],
                    [1571511829,"1.8359329"],
                    [1571511830,"1.8359330"],
                    [1571511831,"1.8359331"]
                  ]
                }]
              }
          }"#,
        );
        let res1_json = parse_json(&test1_json);
        assert_eq!(res1_json.is_some(), true);
        let res1_load = test.load_prometheus_response(res1_json.unwrap());
        // 5 items should have been loaded, 5 already existed.
        assert_eq!(res1_load, Ok(5usize));
        assert_eq!(test.series.active_items, 10usize);
    }
}
