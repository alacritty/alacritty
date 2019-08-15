//! Exports the TimeSeries class
//! The TimeSeries is a circular buffer that contains an entry per epoch
//! at different granularities. It is maintained as a Vec<(u64, T)> where
//! T is a metric. Since metrics will overwrite the contents of the array
//! partially, the start of the metrics and the end of the metrics are
//! maintained as two separate indexes. This allows the vector to shrink
//! and rotate without relocation of memory or shifting of the vector.

// DONE:
// -- Add step to query (1 second resolution for example)
// -- Add min/max time to query.
// -- Move to config.yaml
// -- The yaml should drive an array of activity dashboards
// -- Tokio timers
// -- Use prometheus queries instead of our own aggregation/etc.
// IN PROGRESS:
// -- Logging
// -- Group labels into separate colors (find something that does color spacing in rust)
// TODO:
// -- The dashboards should be toggable, some key combination
// -- When activated on toggle it could blur a portion of the screen
// -- derive builder
// -- mock the prometheus server and response
// -- We should re-use the circular_push for the opengl_vec

extern crate log;
#[macro_use]
extern crate serde_derive;

extern crate futures;
extern crate hyper;
extern crate percent_encoding;
extern crate serde;
extern crate serde_json;
extern crate tokio;
extern crate tokio_core;
// use crate::term::color::Rgb;
// use crate::term::SizeInfo;
use log::*;
use std::time::UNIX_EPOCH;

pub mod async_utils;
pub mod config;
pub mod prometheus;

/// `MissingValuesPolicy` provides several ways to deal with missing values
/// when drawing the Metric
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MissingValuesPolicy {
    Zero,
    One,
    First,
    Last,
    Fixed(f64),
    Avg,
    Max,
    Min,
}

impl Default for MissingValuesPolicy {
    fn default() -> MissingValuesPolicy {
        MissingValuesPolicy::Zero
    }
}

/// `ValueCollisionPolicy` handles collisions when several values are collected
/// for the same time unit, allowing for overwriting, incrementing, etc.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ValueCollisionPolicy {
    Overwrite,
    Increment,
    Decrement,
    Ignore,
}

impl Default for ValueCollisionPolicy {
    fn default() -> ValueCollisionPolicy {
        ValueCollisionPolicy::Increment
    }
}

/// `TimeSeriesStats` contains statistics about the current TimeSeries
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TimeSeriesStats {
    max: f64,
    min: f64,
    avg: f64, // Calculation may lead to overflow
    first: f64,
    last: f64,
    count: usize,
    sum: f64, // May overflow
    is_dirty: bool,
}

impl Default for TimeSeriesStats {
    fn default() -> TimeSeriesStats {
        TimeSeriesStats {
            max: 0f64,
            min: 0f64,
            avg: 0f64,
            first: 0f64,
            last: 0f64,
            count: 0usize,
            sum: 0f64,
            is_dirty: false,
        }
    }
}

/// `TimeSeries` contains a vector of tuple (epoch, Option<value>)
/// The vector behaves as a circular buffer to avoid shifting values.
/// The circular buffer may be invalidated partially, for example when too much
/// time has passed without metrics, the vecotr is allowed to shrink without
/// memory rellocation, this is achieved by using two indexes for the first
/// and last item.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TimeSeries {
    /// Capture events through time
    /// Contains one entry per time unit
    pub metrics: Vec<(u64, Option<f64>)>,

    /// Number of items to store in our metrics vec
    pub metrics_capacity: usize,

    /// Stats for the TimeSeries
    pub stats: TimeSeriesStats,

    /// Useful for records that do not increment but rather are a fixed
    /// or absolute value recorded at a given time
    pub collision_policy: ValueCollisionPolicy,

    /// Missing values returns a value for a specific time there is no data
    /// recorded.
    pub missing_values_policy: MissingValuesPolicy,

    /// The first item in the circular buffer
    pub first_idx: usize,

    /// The last item in the circular buffer
    pub last_idx: usize,

    /// The circular buffer has two indexes, if the start and end
    /// indexes are the same, then the buffer is full or has one item
    /// By knowing the active_items in advance we know which situation is true
    pub active_items: usize,
}

/// `IterTimeSeries` provides the Iterator Trait for TimeSeries metrics.
/// The state for the iteration is held en "pos" field. The "current_item" is
/// used to determine if further iterations on the circular buffer is needed.
pub struct IterTimeSeries<'a> {
    /// The reference to the TimeSeries struct to iterate over.
    inner: &'a TimeSeries,
    /// The current position state
    pos: usize,
    /// The current item number, to be compared with the active_items
    current_item: usize,
}

/// `ReferencePointDecoration` draws a fixed point to give a reference point
/// of what a drawn value may mean
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ReferencePointDecoration {
    /// The value at which to draw the reference point
    pub value: f64,

    /// The reference point will use additional height for the axis line
    /// this makes it fit in the configured space, basically the value
    /// will be incremented by this additional percentage to give more
    /// space to draw the axis tick
    #[serde(default)]
    pub height_multiplier: f64,

    /// hexadecimal color
    #[serde(default)]
    pub color: String,

    /// Transparency
    #[serde(default)]
    pub alpha: f32,

    /// The pixels to separate from the left and right
    #[serde(default)]
    pub padding: Value2D,

    /// The opengl vertices is stored in this vector
    /// The capacity is always 12, see opengl_vertices()
    #[serde(default)]
    pub opengl_data: Vec<f32>,
}

impl Default for ReferencePointDecoration {
    fn default() -> ReferencePointDecoration {
        ReferencePointDecoration {
            value: 1.0,
            height_multiplier: 0.05,
            color: String::from("0xff0000"),
            alpha: 1.0,
            padding: Value2D {
                x: 1f32,
                y: 0f32, // No top/bottom padding
            },
            opengl_data: vec![],
        }
    }
}

impl ReferencePointDecoration {
    /// `opengl_vertices` Scales the Marker Line to the current size of
    /// the displayed points
    pub fn opengl_vertices(&self) -> Vec<f32> {
        self.opengl_data.clone()
    }

    /// `top_value` increments the reference point value by an additional
    /// percentage to account for space to draw the axis tick
    pub fn top_value(&self) -> f64 {
        self.value + self.value * self.height_multiplier
    }

    /// `bottom_value` decrements the reference point value by a percentage
    /// to account for space to draw the axis tick
    pub fn bottom_value(&self) -> f64 {
        self.value - self.value * self.height_multiplier
    }

    /// `update_opengl_vecs` Draws a marker at a fixed position for
    /// reference.
    pub fn update_opengl_vecs(
        &mut self,
        display_size: SizeInfo,
        offset: Value2D,
        chart_max_value: f64,
    ) {
        debug!("ReferencePointDecoration: Starting update_opengl_vecs");
        if 12 != self.opengl_data.capacity() {
            self.opengl_data = vec![0.; 12];
        }
        // The vertexes of the above marker idea can be represented as
        // connecting lines for these coordinates:
        //         |Actual Draw Metric Data|
        // x1,y2   |                       |   x2,y2
        // x1,y1 --|-----------------------|-- x2,y1
        // x1,y3   |                       |   x2,y3
        // |- 10% -|-         80%         -|- 10% -|
        // TODO: Add marker_line color to opengl
        // TODO: Call only when max or min have changed in collected metrics
        //
        // Calculate X coordinates:
        let x1 = display_size.scale_x(offset.x);
        let x2 = display_size.scale_x(offset.x + display_size.chart_width);

        // Calculate Y, the marker hints are 10% of the current values
        // This means that the
        let y1 = display_size.scale_y(chart_max_value, self.value);
        let y2 = display_size.scale_y(chart_max_value, self.top_value());
        let y3 = display_size.scale_y(chart_max_value, self.bottom_value());

        // Build the left most axis "tick" mark.
        self.opengl_data[0] = x1;
        self.opengl_data[1] = y2;
        self.opengl_data[2] = x1;
        self.opengl_data[3] = y3;

        // Create the line to the other side
        self.opengl_data[4] = x1;
        self.opengl_data[5] = y1;
        self.opengl_data[6] = x2;
        self.opengl_data[7] = y1;
        // Finish the axis "tick" on the other side
        self.opengl_data[8] = x2;
        self.opengl_data[9] = y3;
        self.opengl_data[10] = x2;
        self.opengl_data[11] = y2;
        debug!("ReferencePointDecoration: Finished update_opengl_vecs: {:?}", self.opengl_data);
    }
}

/// `Decoration` contains several types of decorations to add to a chart
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type")]
pub enum Decoration {
    #[serde(rename = "reference")]
    Reference(ReferencePointDecoration),
    None,
    /* Maybe add Average, threshold coloring (turn line red after a certain
     * point) */
}

impl Default for Decoration {
    fn default() -> Decoration {
        Decoration::None
    }
}

impl Decoration {
    /// `width` of the Decoration as it may need space to be drawn, otherwise
    /// the decoration and the data itself would overlap, these are pixels
    fn width(&self) -> f32 {
        match self {
            Decoration::Reference(d) => d.padding.x,
            Decoration::None => 0f32,
        }
    }

    /// `top_value` is the Y value of the decoration, it needs to be
    /// in the range of the metrics that have been collected, thus f64
    /// this is the highest point the Decoration will use
    fn top_value(&self) -> f64 {
        match self {
            Decoration::Reference(ref d) => d.top_value(),
            Decoration::None => 0f64,
        }
    }

    /// `bottom_value` is the Y value of the decoration, it needs to be
    /// in the range of the metrics that have been collected, thus f64
    /// this is the lowest point the Decoration will use
    fn bottom_value(&self) -> f64 {
        match self {
            Decoration::Reference(d) => d.value - d.value * d.height_multiplier,
            Decoration::None => 0f64,
        }
    }

    /// `update_opengl_vecs` calls the decoration update methods
    fn update_opengl_vecs(
        &mut self,
        display_size: SizeInfo,
        offset: Value2D,
        chart_max_value: f64,
    ) {
        match self {
            Decoration::Reference(ref mut d) => {
                d.update_opengl_vecs(display_size, offset, chart_max_value)
            },
            Decoration::None => (),
        }
    }

    /// `opengl_vertices` returns the representation of the decoration in
    /// opengl. These are for now GL_LINES and 2D
    pub fn opengl_vertices(&self) -> Vec<f32> {
        match self {
            Decoration::Reference(d) => d.opengl_vertices(),
            Decoration::None => vec![],
        }
    }
}

/// `ManualTimeSeries` is a 2D struct from top left being 0,0
/// and bottom right being display limits in pixels
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ManualTimeSeries {
    /// The name of the ManualTimeSeries
    pub name: String,

    /// The TimeSeries that contains the data
    #[serde(default)]
    pub series: TimeSeries,

    /// The granularity to store
    #[serde(default)]
    pub granularity: u64,

    /// The color of the TimeSeries
    #[serde(default)]
    pub color: String,

    /// The transparency of the TimeSeries
    #[serde(default)]
    pub alpha: f32,
}

impl Default for ManualTimeSeries {
    fn default() -> ManualTimeSeries {
        ManualTimeSeries {
            name: String::from("unkown"),
            series: TimeSeries::default(),
            granularity: 1, // 1 second
            color: String::from("0x00ff00"),
            alpha: 1.0,
        }
    }
}

/// `TimeSeriesSource` contains several types of time series that can be extended
/// with drawable data
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type")]
pub enum TimeSeriesSource {
    #[serde(rename = "prometheus")]
    PrometheusTimeSeries(prometheus::PrometheusTimeSeries),
    #[serde(rename = "alacritty_input")]
    AlacrittyInput(ManualTimeSeries),
    #[serde(rename = "alacritty_output")]
    AlacrittyOutput(ManualTimeSeries),
    #[serde(rename = "async_items_loaded")]
    AsyncLoadedItems(ManualTimeSeries),
}

impl Default for TimeSeriesSource {
    fn default() -> TimeSeriesSource {
        TimeSeriesSource::AlacrittyInput(ManualTimeSeries::default())
    }
}

impl TimeSeriesSource {
    pub fn series(&self) -> TimeSeries {
        match self {
            TimeSeriesSource::PrometheusTimeSeries(x) => x.series.clone(),
            TimeSeriesSource::AlacrittyInput(x) => x.series.clone(),
            TimeSeriesSource::AlacrittyOutput(x) => x.series.clone(),
            TimeSeriesSource::AsyncLoadedItems(x) => x.series.clone(),
        }
    }

    pub fn series_mut(&mut self) -> &mut TimeSeries {
        match self {
            TimeSeriesSource::PrometheusTimeSeries(x) => &mut x.series,
            TimeSeriesSource::AlacrittyInput(x) => &mut x.series,
            TimeSeriesSource::AlacrittyOutput(x) => &mut x.series,
            TimeSeriesSource::AsyncLoadedItems(x) => &mut x.series,
        }
    }

    pub fn name(&self) -> String {
        match self {
            TimeSeriesSource::PrometheusTimeSeries(x) => x.name.clone(),
            TimeSeriesSource::AlacrittyInput(x) => x.name.clone(),
            TimeSeriesSource::AlacrittyOutput(x) => x.name.clone(),
            TimeSeriesSource::AsyncLoadedItems(x) => x.name.clone(),
        }
    }

    // XXX: SEB: This is really ugly, we should have maybe Trait for Drawable and have a color
    // easily available or have like a .prop("color").
    pub fn color(&self) -> String {
        match self {
            TimeSeriesSource::PrometheusTimeSeries(x) => x.color.clone(),
            TimeSeriesSource::AlacrittyInput(x) => x.color.clone(),
            TimeSeriesSource::AlacrittyOutput(x) => x.color.clone(),
            TimeSeriesSource::AsyncLoadedItems(x) => x.color.clone(),
        }
    }

    pub fn alpha(&self) -> f32 {
        match self {
            TimeSeriesSource::PrometheusTimeSeries(x) => x.alpha,
            TimeSeriesSource::AlacrittyInput(x) => x.alpha,
            TimeSeriesSource::AlacrittyOutput(x) => x.alpha,
            TimeSeriesSource::AsyncLoadedItems(x) => x.alpha,
        }
    }
}

/// `Value2D` provides X,Y values for several uses, such as offset, padding
#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub struct Value2D {
    #[serde(default)]
    x: f32,
    #[serde(default)]
    y: f32,
}

/// `SizeInfo` is a copy of the Alacritty SizeInfo, XXX: remove on merge.
#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub struct SizeInfo {
    pub width: f32,
    pub height: f32,
    pub chart_width: f32,
    pub chart_height: f32,
    pub cell_width: f32,
    pub cell_height: f32,
    pub padding_x: f32,
    pub padding_y: f32,
}

impl SizeInfo {
    /// `scale_x` Scales the value from the current display boundary to
    /// a cartesian plane from [-1.0, 1.0], where -1.0 is 0px (left-most) and
    /// 1.0 is the `display_width` parameter (right-most), i.e. 1024px.
    pub fn scale_x(&self, input_value: f32) -> f32 {
        let center_x = self.width / 2.;
        let x = self.padding_x + input_value;
        (x - center_x) / center_x
    }

    /// `scale_y_to_size` Scales the value from the current display boundary to
    /// a cartesian plane from [-1.0, 1.0], where 1.0 is 0px (top) and -1.0 is
    /// the `display_height` parameter (bottom), i.e. 768px.
    pub fn scale_y(&self, max_value: f64, input_value: f64) -> f32 {
        let center_y = self.height / 2.;
        let y = self.height
            - 2. * self.padding_y
            - (self.chart_height * (input_value as f32 / max_value as f32));
        -(y - center_y) / center_y
    }
}

/// `TimeSeriesChart` has an array of TimeSeries to display, it contains the
/// X, Y position and has methods to draw in opengl.
#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct TimeSeriesChart {
    /// The name of the Chart
    pub name: String,

    /// The different sources of the TimeSeries to draw
    #[serde(rename = "series")]
    pub sources: Vec<TimeSeriesSource>,

    /// Decorations such as color, transparency, etc
    #[serde(default)]
    pub decorations: Vec<Decoration>,

    /// The merged stats of the TimeSeries
    #[serde(default)]
    pub stats: TimeSeriesStats,

    /// The offset in which the activity line should be drawn
    #[serde(default)]
    pub offset: Value2D,

    /// The width of the activity chart/histogram
    #[serde(default)]
    pub width: f32,

    /// The height of the activity line region
    #[serde(default)]
    pub height: f32,

    /// The spacing between the activity line segments, could be renamed to line length
    #[serde(default)]
    pub tick_spacing: f32,

    /// The opengl representation of the each series.
    #[serde(default)]
    pub opengl_vecs: Vec<Vec<f32>>,
}

impl TimeSeriesChart {
    /// `update_series_opengl_vecs` Represents the activity levels values in a
    /// drawable vector for opengl, for a specific index in the series array
    pub fn update_series_opengl_vecs(&mut self, series_idx: usize, display_size: SizeInfo) {
        debug!("Chart: Starting update_series_opengl_vecs for series index: {}", series_idx);
        if series_idx > self.sources.len() {
            error!("Request for out of bound series index: {}", series_idx);
            return;
        }
        while self.opengl_vecs.capacity() < self.sources.capacity() {
            self.opengl_vecs.push(vec![]);
        }
        let mut display_size = display_size;
        display_size.chart_height = self.height;
        display_size.chart_width = self.width;
        // Get the opengl representation of the vector
        let opengl_vecs_capacity = self.sources[series_idx].series().active_items;
        if opengl_vecs_capacity > self.opengl_vecs[series_idx].capacity() {
            let missing_capacity = opengl_vecs_capacity - self.opengl_vecs[series_idx].capacity();
            self.opengl_vecs[series_idx].reserve(missing_capacity);
        }
        debug!(
            "Chart: Needed OpenGL capacity: {}, Display Size: {:?}, offset {:?}",
            opengl_vecs_capacity, display_size, self.offset,
        );
        for source in &mut self.sources {
            if source.series().stats.is_dirty {
                debug!("Chart: '{}' stats are dirty, needs recalculating", source.name());
                source.series_mut().calculate_stats();
            }
        }
        self.calculate_stats();
        let mut decorations_space = 0f32;
        for decoration in &self.decorations {
            debug!("Chart: Adding width of decoration: {}", decoration.width());
            decorations_space += decoration.width();
        }
        debug!("Chart: width: {}, decorations_space: {}", self.width, decorations_space);
        let missing_values_fill = self.sources[series_idx].series().get_missing_values_fill();
        debug!(
            "Chart: Using {} to fill missing values. Metrics capacity: {}",
            missing_values_fill,
            self.sources[series_idx].series().metrics_capacity
        );
        let tick_spacing = (self.width - decorations_space)
            / self.sources[series_idx].series().metrics_capacity as f32;
        debug!("Chart: Using tick_spacing {}", tick_spacing);
        for (idx, metric) in self.sources[series_idx].series().iter().enumerate() {
            // The decorations width request is on both left and right.
            let x_value = idx as f32 * tick_spacing + (decorations_space / 2f32);
            // If there is a Marker Line, it takes 10% of the initial horizontal space
            let y_value = match metric.1 {
                Some(x) => x,
                None => missing_values_fill,
            };
            let scaled_x = display_size.scale_x(x_value + self.offset.x);
            let scaled_y = display_size.scale_y(self.stats.max, y_value);
            // Adding twice to a vec, could this be made into one operation? Is this slow?
            // need to transform activity line values from varying levels into scaled [-1, 1]
            // XXX: Move to Circular Buffer
            if (idx + 1) * 2 > self.opengl_vecs[series_idx].len() {
                self.opengl_vecs[series_idx].push(scaled_x);
                self.opengl_vecs[series_idx].push(scaled_y);
            } else {
                self.opengl_vecs[series_idx][idx * 2] = scaled_x;
                self.opengl_vecs[series_idx][idx * 2 + 1] = scaled_y;
            }
        }
        for decoration in &mut self.decorations {
            debug!("Chart: Updating decoration {:?} vertices", decoration);
            decoration.update_opengl_vecs(display_size, self.offset, self.stats.max);
        }
    }

    /// `update_all_series_opengl_vecs` Represents the activity levels values in a
    /// drawable vector for opengl for all the available series in the current chart
    pub fn update_all_series_opengl_vecs(&mut self, display_size: SizeInfo) {
        debug!("Chart: Starting update_all_series_opengl_vecs");
        for idx in 0..self.sources.len() {
            self.update_series_opengl_vecs(idx, display_size);
        }
    }

    /// `calculate_stats` Iterates over the time series stats and merges them.
    /// This will also go through the decorations and account for the requested
    /// draw space for them.
    pub fn calculate_stats(&mut self) {
        let mut max_activity_value = std::f64::MIN;
        let mut min_activity_value = std::f64::MAX;
        let mut sum_activity_values = 0f64;
        let mut filled_stats = 0usize;
        for source in &mut self.sources {
            if source.series_mut().stats.is_dirty {
                source.series_mut().calculate_stats();
            }
        }
        for source in &self.sources {
            if source.series().stats.max > max_activity_value {
                max_activity_value = source.series().stats.max;
            }
            if source.series().stats.min < min_activity_value {
                min_activity_value = source.series().stats.min;
            }
            sum_activity_values += source.series().stats.sum;
            for series_idx in source.series().as_vec() {
                if series_idx.1.is_some() {
                    filled_stats += 1;
                }
            }
        }
        // Account for the decoration requested height
        for decoration in &self.decorations {
            let top_value = decoration.top_value();
            let bottom_value = decoration.bottom_value();
            if top_value > max_activity_value {
                max_activity_value = top_value
            }
            if bottom_value < min_activity_value {
                min_activity_value = bottom_value;
            }
        }
        self.stats.max = max_activity_value;
        self.stats.min = min_activity_value;
        self.stats.sum = sum_activity_values;
        self.stats.avg = sum_activity_values / filled_stats as f64;
        self.stats.is_dirty = false;
        debug!("Chart: Updated statistics to: {:?}, filled_stats: {:?}", self.stats, filled_stats);
    }
}

impl Default for TimeSeries {
    fn default() -> TimeSeries {
        // This leads to 5 mins of metrics to show by default.
        let default_capacity = 300usize;
        TimeSeries {
            metrics_capacity: default_capacity,
            metrics: Vec::with_capacity(default_capacity),
            stats: TimeSeriesStats::default(),
            collision_policy: ValueCollisionPolicy::default(),
            missing_values_policy: MissingValuesPolicy::default(),
            first_idx: 0,
            last_idx: 0,
            active_items: 0,
        }
    }
}

impl TimeSeries {
    /// `with_capacity` builder changes the amount of metrics in the vec
    pub fn with_capacity(self, n: usize) -> TimeSeries {
        let mut new_self = self;
        new_self.metrics = Vec::with_capacity(n);
        new_self.metrics_capacity = n;
        new_self
    }

    /// `with_missing_values_policy` receives a String and returns
    /// a MissingValuesPolicy, TODO: the "Fixed" value is not implemented.
    pub fn with_missing_values_policy(mut self, policy_type: String) -> TimeSeries {
        self.missing_values_policy = match policy_type.as_ref() {
            "zero" => MissingValuesPolicy::Zero,
            "one" => MissingValuesPolicy::One,
            "min" => MissingValuesPolicy::Min,
            "max" => MissingValuesPolicy::Max,
            "last" => MissingValuesPolicy::Last,
            "avg" => MissingValuesPolicy::Avg,
            "first" => MissingValuesPolicy::First,
            _ => {
                // TODO: Implement FromStr somehow
                MissingValuesPolicy::Zero
            },
        };
        self
    }

    /// `calculate_stats` Iterates over the metrics and sets the stats
    pub fn calculate_stats(&mut self) {
        // Recalculating seems to be necessary because we are constantly
        // moving items out of the Vec<> so our cache can easily get out of
        // sync
        let mut max_activity_value = std::f64::MIN;
        let mut min_activity_value = std::f64::MAX;
        let mut sum_activity_values = 0f64;
        let mut filled_metrics = 0usize;
        for entry in self.iter() {
            if let Some(metric) = entry.1 {
                if metric > max_activity_value {
                    max_activity_value = metric;
                }
                if metric < min_activity_value {
                    min_activity_value = metric;
                }
                sum_activity_values += metric;
                filled_metrics += 1;
            }
        }
        self.stats.max = max_activity_value;
        self.stats.min = min_activity_value;
        self.stats.sum = sum_activity_values;
        self.stats.avg = sum_activity_values / (filled_metrics as f64);
        self.stats.is_dirty = false;
    }

    /// `get_missing_values_fill` uses the MissingValuesPolicy to decide
    /// which value to place on empty metric timeslots when drawing
    pub fn get_missing_values_fill(&self) -> f64 {
        match self.missing_values_policy {
            MissingValuesPolicy::Zero => 0f64,
            MissingValuesPolicy::One => 1f64,
            MissingValuesPolicy::Min => self.stats.min,
            MissingValuesPolicy::Max => self.stats.max,
            MissingValuesPolicy::Last => self.get_last_filled(),
            MissingValuesPolicy::First => self.get_first_filled(),
            MissingValuesPolicy::Avg => self.stats.avg,
            MissingValuesPolicy::Fixed(val) => val,
        }
    }

    /// `resolve_metric_collision` ensures the policy for colliding values is
    /// applied.
    pub fn resolve_metric_collision(&self, existing: f64, new: f64) -> f64 {
        match self.collision_policy {
            ValueCollisionPolicy::Increment => existing + new,
            ValueCollisionPolicy::Overwrite => new,
            ValueCollisionPolicy::Decrement => existing - new,
            ValueCollisionPolicy::Ignore => existing,
        }
    }

    /// `circular_push` an item to the circular buffer
    pub fn circular_push(&mut self, input: (u64, Option<f64>)) {
        if self.metrics.len() < self.metrics_capacity {
            self.metrics.push(input);
            self.active_items += 1;
        } else {
            // The vector might have been invalidated because data was outdated.
            // The first and last index shorten the vector but leave old data
            // still
            if self.first_idx == 0 && self.last_idx < self.metrics_capacity {
                self.metrics[self.last_idx] = input;
            } else {
                self.metrics[self.first_idx] = input;
                self.first_idx = (self.first_idx + 1) % self.metrics_capacity;
            }
            if self.first_idx + self.last_idx < self.metrics_capacity {
                self.active_items += 1;
            }
        }
        self.stats.is_dirty = true;
        self.last_idx = (self.last_idx + 1) % (self.metrics_capacity + 1);
    }

    /// `push` Adds values to the circular buffer adding empty entries for
    /// missing entries, may invalidate the buffer if all data is outdated
    pub fn push(&mut self, input: (u64, f64)) {
        if !self.metrics.is_empty() {
            let last_idx = if self.last_idx == self.metrics_capacity || self.last_idx == 0 {
                self.metrics.len() - 1
            } else {
                self.last_idx - 1
            };
            let inactive_time = if input.0 > self.metrics[last_idx].0 {
                (input.0 - self.metrics[last_idx].0) as usize
            } else {
                0usize
            };
            if inactive_time > self.metrics_capacity {
                // The whole vector should be discarded
                self.first_idx = 0;
                self.last_idx = 1;
                self.metrics[0] = (input.0, Some(input.1));
                self.active_items = 1;
            } else if inactive_time == 0 {
                // In this case, the last epoch and the current epoch match
                if let Some(curr_val) = self.metrics[last_idx].1 {
                    self.metrics[last_idx].1 =
                        Some(self.resolve_metric_collision(curr_val, input.1));
                } else {
                    self.metrics[last_idx].1 = Some(input.1);
                }
            } else {
                // Fill missing entries with None
                let max_epoch = self.metrics[last_idx].0;
                for fill_epoch in (max_epoch + 1)..input.0 {
                    self.circular_push((fill_epoch, None));
                }
                self.circular_push((input.0, Some(input.1)));
            }
        } else {
            self.circular_push((input.0, Some(input.1)));
        }
    }

    /// `get_last_filled` Returns the last filled entry in the circular buffer
    pub fn get_last_filled(&self) -> f64 {
        let mut idx = if self.last_idx == self.metrics_capacity { 0 } else { self.last_idx - 1 };
        loop {
            if let Some(res) = self.metrics[idx].1 {
                return res;
            }
            idx = if idx == 0 { self.metrics.len() } else { idx - 1 };
            if idx == self.first_idx {
                break;
            }
        }
        0f64
    }

    /// `get_first_filled` Returns the first filled entry in the circular buffer
    pub fn get_first_filled(&self) -> f64 {
        for entry in self.iter() {
            if let Some(metric) = entry.1 {
                return metric;
            }
        }
        0f64
    }

    /// `as_vec` Returns the circular buffer in flat vec format
    // ....[c]
    // ..[b].[d]
    // [a].....[e]
    // ..[h].[f]
    // ....[g]
    // first_idx = "^"
    // last_idx  = "v"
    // [a][b][c][d][e][f][g][h]
    //  0  1  2  3  4  5  6  7
    //  ^v                        # empty
    //  ^  v                      # 0
    //  ^                       v # vec full
    //  v                    ^    # 7
    pub fn as_vec(&self) -> Vec<(u64, Option<f64>)> {
        if self.metrics.is_empty() {
            return vec![];
        }
        let mut res: Vec<(u64, Option<f64>)> = Vec::with_capacity(self.metrics_capacity);
        for entry in self.iter() {
            res.push(entry.clone());
        }
        res
    }

    pub fn push_current_epoch(&mut self, input: f64) {
        let now = std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        self.push((now, input));
    }

    // `iter` Returns an Iterator from the current start.
    fn iter(&self) -> IterTimeSeries {
        IterTimeSeries { inner: self, pos: self.first_idx, current_item: 0 }
    }
}

impl<'a> Iterator for IterTimeSeries<'a> {
    type Item = &'a (u64, Option<f64>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.inner.metrics.is_empty() || self.current_item == self.inner.active_items {
            return None;
        }
        let curr_pos = self.pos % self.inner.metrics.len();
        self.pos = (self.pos + 1) % (self.inner.metrics.len() + 1);
        self.current_item += 1;
        Some(&self.inner.metrics[curr_pos])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_log() {
        let _ = env_logger::builder().is_test(true).try_init();
    }
    #[test]
    fn it_pushes_circular_buffer() {
        // The circular buffer inserts rotating the first and last index
        let mut test = TimeSeries::default().with_capacity(4);
        test.circular_push((10, Some(0f64)));
        assert_eq!(test.first_idx, 0);
        assert_eq!(test.last_idx, 1);
        test.circular_push((11, Some(1f64)));
        test.circular_push((12, None));
        test.circular_push((13, Some(3f64)));
        assert_eq!(test.first_idx, 0);
        assert_eq!(test.last_idx, 4);
        assert_eq!(test.metrics, vec![
            (10, Some(0f64)),
            (11, Some(1f64)),
            (12, None),
            (13, Some(3f64))
        ]);
        test.circular_push((14, Some(4f64)));
        assert_eq!(test.metrics, vec![
            (14, Some(4f64)),
            (11, Some(1f64)),
            (12, None),
            (13, Some(3f64))
        ]);
        assert_eq!(test.first_idx, 1);
        assert_eq!(test.last_idx, 0);
        test.circular_push((15, Some(5f64)));
        assert_eq!(test.metrics, vec![
            (14, Some(4f64)),
            (15, Some(5f64)),
            (12, None),
            (13, Some(3f64))
        ]);
        assert_eq!(test.first_idx, 2);
        assert_eq!(test.last_idx, 1);
    }
    #[test]
    fn it_gets_last_filled_value() {
        let mut test = TimeSeries::default().with_capacity(4);
        // Some values should be inserted as None
        test.push((10, 0f64));
        test.circular_push((11, None));
        test.circular_push((12, None));
        test.circular_push((13, None));
        assert_eq!(test.get_last_filled(), 0f64);
        let mut test = TimeSeries::default().with_capacity(4);
        test.circular_push((11, None));
        test.push((12, 2f64));
    }
    #[test]
    fn it_transforms_to_flat_vec() {
        let mut test = TimeSeries::default().with_capacity(4);
        // Some values should be inserted as None
        assert_eq!(test.first_idx, 0);
        assert_eq!(test.last_idx, 0);
        test.push((10, 0f64));
        assert_eq!(test.first_idx, 0);
        assert_eq!(test.last_idx, 1);
        test.push((13, 3f64));
        assert_eq!(test.first_idx, 0);
        assert_eq!(test.last_idx, 4);
        assert_eq!(test.as_vec(), vec![(10, Some(0f64)), (11, None), (12, None), (13, Some(3f64))]);
        test.push((14, 4f64));
        // Starting at 11
        test.first_idx = 1;
        test.last_idx = 1;
        assert_eq!(test.as_vec(), vec![(11, None), (12, None), (13, Some(3f64)), (14, Some(4f64))]);
        // Only 11
        test.active_items = 1;
        test.first_idx = 1;
        test.last_idx = 2;
        assert_eq!(test.as_vec(), vec![(11, None)]);
        // Only 13
        test.first_idx = 3;
        test.last_idx = 4;
        test.active_items = 1;
        assert_eq!(test.as_vec(), vec![(13, Some(3f64))]);
        // 13, 14
        test.first_idx = 3;
        test.last_idx = 1;
        test.active_items = 2;
        assert_eq!(test.as_vec(), vec![(13, Some(3f64)), (14, Some(4f64))]);
    }
    #[test]
    fn it_fills_empty_epochs() {
        let mut test = TimeSeries::default().with_capacity(4);
        // Some values should be inserted as None
        test.push((10, 0f64));
        test.push((13, 3f64));
        assert_eq!(test.metrics, vec![(10, Some(0f64)), (11, None), (12, None), (13, Some(3f64))]);
        assert_eq!(test.active_items, 4);
        // Test the whole vector is discarded
        test.push((18, 8f64));
        assert_eq!(test.active_items, 1);
        assert_eq!(test.metrics, vec![(18, Some(8f64)), (11, None), (12, None), (13, Some(3f64))]);
        assert_eq!(test.first_idx, 0);
        assert_eq!(test.last_idx, 1);
        assert_eq!(test.active_items, 1);
        assert_eq!(test.as_vec(), vec![(18, Some(8f64))]);
        test.push((20, 0f64));
        assert_eq!(test.metrics, vec![
            (18, Some(8f64)),
            (19, None),
            (20, Some(0f64)),
            (13, Some(3f64))
        ]);
        assert_eq!(test.first_idx, 0);
        assert_eq!(test.last_idx, 3);
        assert_eq!(test.active_items, 3);
        assert_eq!(test.as_vec(), vec![(18, Some(8f64)), (19, None), (20, Some(0f64))]);
        test.push((50, 5f64));
        assert_eq!(
            test.metrics,
            // Many outdated entries
            vec![(50, Some(5f64)), (19, None), (20, Some(0f64)), (13, Some(3f64))]
        );
        assert_eq!(test.as_vec(), vec![(50, Some(5f64))]);
        test.push((53, 3f64));
        assert_eq!(test.metrics, vec![(50, Some(5f64)), (51, None), (52, None), (53, Some(3f64))]);
    }
    #[test]
    fn it_applies_missing_policies() {
        let mut test_zero = TimeSeries::default().with_capacity(5);
        let mut test_one =
            TimeSeries::default().with_capacity(5).with_missing_values_policy("one".to_string());
        let mut test_min =
            TimeSeries::default().with_capacity(5).with_missing_values_policy("min".to_string());
        let mut test_max =
            TimeSeries::default().with_capacity(5).with_missing_values_policy("max".to_string());
        let mut test_last =
            TimeSeries::default().with_capacity(5).with_missing_values_policy("last".to_string());
        let mut test_first =
            TimeSeries::default().with_capacity(5).with_missing_values_policy("first".to_string());
        let mut test_avg =
            TimeSeries::default().with_capacity(5).with_missing_values_policy("avg".to_string());
        test_zero.push((0, 9f64));
        test_zero.push((2, 1f64));
        test_one.push((0, 9f64));
        test_one.push((2, 1f64));
        test_min.push((0, 9f64));
        test_min.push((2, 1f64));
        test_max.push((0, 9f64));
        test_max.push((2, 1f64));
        test_last.push((0, 9f64));
        test_last.push((2, 1f64));
        test_first.push((0, 9f64));
        test_first.push((2, 1f64));
        test_avg.push((0, 9f64));
        test_avg.push((2, 1f64));
        test_zero.calculate_stats();
        test_one.calculate_stats();
        test_min.calculate_stats();
        test_max.calculate_stats();
        test_last.calculate_stats();
        test_first.calculate_stats();
        test_avg.calculate_stats();
        assert_eq!(test_zero.get_missing_values_fill(), 0f64);
        assert_eq!(test_one.get_missing_values_fill(), 1f64);
        assert_eq!(test_min.get_missing_values_fill(), 1f64);
        assert_eq!(test_max.get_missing_values_fill(), 9f64);
        assert_eq!(test_last.get_missing_values_fill(), 1f64);
        assert_eq!(test_first.get_missing_values_fill(), 9f64);
        assert_eq!(test_avg.get_missing_values_fill(), 5f64);
        // TODO: add Fixed value test
    }
    #[test]
    fn it_iterates_trait() {
        // Iterator Trait
        // Test an empty TimeSeries vec
        let test0: TimeSeries = TimeSeries::default().with_capacity(4);
        let mut iter_test0 = test0.iter();
        assert_eq!(iter_test0.pos, 0);
        assert!(iter_test0.next().is_none());
        assert!(iter_test0.next().is_none());
        assert_eq!(iter_test0.pos, 0);
        // Simple test with one item
        let mut test1 = TimeSeries::default().with_capacity(4);
        test1.circular_push((10, Some(0f64)));
        let mut iter_test1 = test1.iter();
        assert_eq!(iter_test1.next(), Some(&(10, Some(0f64))));
        assert_eq!(iter_test1.pos, 1);
        assert!(iter_test1.next().is_none());
        assert!(iter_test1.next().is_none());
        assert_eq!(iter_test1.pos, 1);
        // Simple test with 3 items, rotated to start first item and 2nd
        // position and last item at 3rd position
        let mut test2 = TimeSeries::default().with_capacity(4);
        test2.circular_push((10, Some(0f64)));
        test2.circular_push((11, Some(1f64)));
        test2.circular_push((12, Some(2f64)));
        test2.circular_push((13, Some(3f64)));
        test2.first_idx = 1;
        test2.last_idx = 3;
        assert_eq!(test2.metrics, vec![
            (10, Some(0f64)),
            (11, Some(1f64)),
            (12, Some(2f64)),
            (13, Some(3f64))
        ]);
        let mut iter_test2 = test2.iter();
        assert_eq!(iter_test2.pos, 1);
        assert_eq!(iter_test2.next(), Some(&(11, Some(1f64))));
        assert_eq!(iter_test2.next(), Some(&(12, Some(2f64))));
        assert_eq!(iter_test2.pos, 3);
        // A vec that is completely full
        let mut test3 = TimeSeries::default().with_capacity(4);
        test3.circular_push((10, Some(0f64)));
        test3.circular_push((11, Some(1f64)));
        test3.circular_push((12, Some(2f64)));
        test3.circular_push((13, Some(3f64)));
        {
            let mut iter_test3 = test3.iter();
            assert_eq!(iter_test3.next(), Some(&(10, Some(0f64))));
            assert_eq!(iter_test3.next(), Some(&(11, Some(1f64))));
            assert_eq!(iter_test3.next(), Some(&(12, Some(2f64))));
            assert_eq!(iter_test3.next(), Some(&(13, Some(3f64))));
            assert!(iter_test3.next().is_none());
            assert!(iter_test3.next().is_none());
            assert_eq!(iter_test2.pos, 3);
        }
        // After changing the data the idx is recreatehd at 11 as expected
        test3.circular_push((14, Some(4f64)));
        let mut iter_test3 = test3.iter();
        assert_eq!(iter_test3.next(), Some(&(11, Some(1f64))));
    }

    #[test]
    fn it_scales_x_to_display_size() {
        let mut test = SizeInfo {
            padding_x: 0.,
            padding_y: 0.,
            height: 100.,
            width: 100.,
            ..SizeInfo::default()
        };
        // display size: 100 px, input the value: 0, padding_x: 0
        // The value should return should be left-most: -1.0
        let min = test.scale_x(0f32);
        assert_eq!(min, -1.0f32);
        // display size: 100 px, input the value: 100, padding_x: 0
        // The value should return should be right-most: 1.0
        let max = test.scale_x(100f32);
        assert_eq!(max, 1.0f32);
        // display size: 100 px, input the value: 50, padding_x: 0
        // The value should return should be the center: 0.0
        let mid = test.scale_x(50f32);
        assert_eq!(mid, 0.0f32);
        test.padding_x = 50.;
        // display size: 100 px, input the value: 50, padding_x: 50px
        // The value returned should be the right-most: 1.0
        let mid = test.scale_x(50f32);
        assert_eq!(mid, 1.0f32);
    }

    #[test]
    fn it_scales_y_to_display_size() {
        let mut size_test = SizeInfo {
            padding_x: 0.,
            padding_y: 0.,
            height: 100.,
            chart_height: 100.,
            ..SizeInfo::default()
        };
        let mut chart_test = TimeSeriesChart::default();
        // To make testing easy, let's make three values equivalent:
        // - Chart height
        // - Max Metric collected
        // - Max resolution in pixels
        chart_test.stats.max = 100f64;
        // display size: 100 px, input the value: 0, padding_y: 0
        // The value should return should be lowest: -1.0
        let min = size_test.scale_y(100f64, 0f64);
        assert_eq!(min, -1.0f32);
        // display size: 100 px, input the value: 100, padding_y: 0
        // The value should return should be upper-most: 1.0
        let max = size_test.scale_y(100f64, 100f64);
        assert_eq!(max, 1.0f32);
        // display size: 100 px, input the value: 50, padding_y: 0
        // The value should return should be the center: 0.0
        let mid = size_test.scale_y(100f64, 50f64);
        assert_eq!(mid, 0.0f32);
        size_test.padding_y = 25.;
        // display size: 100 px, input the value: 50, padding_y: 25
        // The value returned should be upper-most: 1.0
        // In this case, the chart (100px) is bigger than the display,
        // which means some values would have been chopped (anything above
        // 50f32)
        let mid = size_test.scale_y(100f64, 50f64);
        assert_eq!(mid, 1.0f32);
    }

    fn simple_chart_setup_with_none() -> (SizeInfo, TimeSeriesChart) {
        init_log();
        let size_test = SizeInfo {
            padding_x: 0.,
            padding_y: 0.,
            height: 200., // Display Height: 200px test
            width: 200.,  // Display Width: 200px test
            ..SizeInfo::default()
        };
        let mut chart_test = TimeSeriesChart::default();
        chart_test.sources.push(TimeSeriesSource::default());
        chart_test.width = 10.;
        chart_test.height = 10.;
        // Test with 10 items only
        // So that every item takes 0.01
        chart_test.sources[0].series_mut().metrics_capacity = 10;
        // |             |   -
        // |             |   |
        // |             |   200
        // |             |   |
        // |XX           |   -
        //
        // |---- 200 ----|
        chart_test.sources[0].series_mut().circular_push((10, Some(0f64)));
        chart_test.sources[0].series_mut().circular_push((11, Some(1f64)));
        chart_test.sources[0].series_mut().circular_push((12, Some(2f64)));
        // Let's make a None value and check the MissingValuesPolicy
        chart_test.sources[0].series_mut().circular_push((14, None));
        // This makes the top value 4
        chart_test.sources[0].series_mut().circular_push((15, Some(4f64)));
        // The current display (10% at the bottom left) should be divided
        // between 4 and 1.
        // metric(4) is -0.9
        // Each metric unit (From 0 to 4) will be 0.025
        // metric(0) is -1.0
        (size_test, chart_test)
    }

    #[test]
    fn it_updates_opengl_vertices() {
        let (size_test, mut chart_test) = simple_chart_setup_with_none();
        chart_test.update_series_opengl_vecs(0, size_test);
        assert_eq!(chart_test.opengl_vecs[0], vec![
            -1.0,   // 1st X value, leftmost.
            -1.0,   // Y value is 0, so -1.0 is the bottom-most
            -0.99,  // X plus 0.01
            -0.975, // Y value is 1, so 25% of the line, so 0.025
            -0.98,  // leftmost plus  0.01 * 2
            -0.95,  // Y value is 2, so 50% from bottom to top
            -0.97,  // leftmost plus 0.01 * 3
            -1.0,   // Top-most value, so the chart height
            -0.96,  // leftmost plus 0.01 * 4, rightmost
            -0.9    // Top-most value, so the chart height
        ]);
    }

    #[test]
    fn it_calculates_reference_point() {
        let (size_test, mut chart_test) = simple_chart_setup_with_none();
        chart_test.decorations.push(Decoration::Reference(ReferencePointDecoration::default()));
        // Calling update_series_opengl_vecs also calls the decoration update opengl vecs
        chart_test.update_series_opengl_vecs(0, size_test);
        let deco_vecs = chart_test.decorations[0].opengl_vertices();

        assert_eq!(chart_test.decorations[0].opengl_vertices().len(), 12);
        // At this point we know 1 unit in the current drawn metrics is equals to
        // 0.025
        assert_eq!(deco_vecs, vec![
            -1.0,     // Left-most
            -0.97375, // 0.25 + 5% height multiplier, so 30% of the line
            -1.0,     // Left-most
            -0.97625, // Y value - 5% height multiplier, so 20% of the line
            -1.0,     // Left-most
            -0.975,   // Y value, so 25% of the line
            -0.9,     // Right-most
            -0.975,   // Y value is 1, so 25% of the line
            -0.9,     // Right-most
            -0.97625, // Y value is 1, so 25% of the line
            -0.9,     // Right-most
            -0.97375, // Y value is 1, so 25% of the line
        ]);
        // Since we have added a Reference point, it needs some space to be
        // drawn, its default width is 1px, turns out to be 0.9 between ticks
        // Also there is an offset of 10 px so divided by 2 (for each side) becomes:
        // 0.05
        assert_eq!(chart_test.opengl_vecs[0], vec![
            -0.995,      // 1st X value, leftmost.
            -1.0,        // Y value is 0, so -1.0 is the bottom-most
            -0.986,      // X plus 0.01
            -0.975,      // Y value is 1, so 25% of the line, so 0.025
            -0.977,      // leftmost plus  0.01 * 2
            -0.95,       // Y value is 2, so 50% from bottom to top
            -0.96800005, // leftmost plus 0.01 * 3
            -1.0,        // Top-most value, so the chart height
            -0.959,      // leftmost plus 0.01 * 4, rightmost
            -0.9         // Top-most value, so the chart height
        ]);
    }
}
// TODO: `init_opengl_context` provides a default initialization of OpengL
// context. This function is called previous to sending the vector data.
// This seems to be part of src/renderer/ mod tho...
// fn init_opengl_context(&self);
// }
