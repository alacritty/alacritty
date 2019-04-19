//! Exports the TimeSeries class

// TODO:
// - Move to the config.yaml
// -- The yaml should drive an array of activity dashboards
// -- The dashboards should be toggable, some key combination
// -- When activated on toggle it could blur a portion of the screen
// -- derive builder
// -- Use prometheus queries instead of our own aggregation/etc.

extern crate futures;
extern crate hyper;
extern crate tokio_core;
extern crate serde_json;
extern crate num_traits;
use crate::term::color::Rgb;
use num_traits::*;
use std::time::UNIX_EPOCH;
use crate::term::SizeInfo;

use std::io;
use futures::{Future, Stream};
use hyper::Client;
use tokio_core::reactor::Core;
use serde_json::Value;

/// `MissingValuesPolicy` provides several ways to deal with missing values
/// to fill the vector of values
#[derive(Debug, Clone)]
pub enum MissingValuesPolicy<T>
where T: Num + Clone + Copy
{
    Zero,
    One,
    First,
    Last,
    Fixed(T),
    Avg,
    Max,
    Min,
}

impl<T> Default for MissingValuesPolicy<T>
where T: Num + Clone + Copy
{
    fn default() -> MissingValuesPolicy<T> {
        MissingValuesPolicy::Zero
    }
}

/// `ValueCollisionPolicy` handles collisions when several values are collected
/// for the same time unit, allowing for overwriting, incrementing, etc.
#[derive(Debug, Clone)]
pub enum ValueCollisionPolicy{
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
#[derive(Debug, Default, Clone)]
pub struct TimeSeriesStats<T>
where T: Num + Clone + Copy
{
    max: T,
    min: T,
    avg: T, // Calculation may lead to overflow
    first: T,
    last: T,
    count: usize,
    sum: T, // May overflow
    is_dirty: bool,
}

/// `TimeSeries` contains a vector of tuple (epoch, value)
#[derive(Default)]
pub struct TimeSeries<T>
where T: Num + Clone + Copy
{
    /// Capture events through time
    /// Contains one entry per second
    pub metrics: Vec<(u64, T)>,

    /// Max activity ticks to show, ties to the activity_levels array
    /// it should cause it to throw away old items for newer records
    pub max_metrics: usize,

    /// Stats for the TimeSeries
    pub metric_stats: TimeSeriesStats<T>,

    /// Useful for records that do not increment but rather are a fixed
    /// or absolute value recorded at a given time
    pub collision_policy: ValueCollisionPolicy,

}

pub struct TimeSeriesChart<T>
where T: Num + Clone + Copy
{
    /// The metrics shown at a given time
    pub metrics: TimeSeries<T>,

    /// A marker line to indicate a reference point, for example for load
    /// to show where the 1 loadavg is, or to show disk capacity
    pub metric_reference: Option<T>,

    /// The offset in which the activity line should be drawn
    pub x_offset: f32,

    /// The width of the activity chart/histogram
    pub width: f32,

    /// The height of the activity line region
    pub metrics_height: f32,

    /// The spacing between the activity line segments, could be renamed to line length
    pub tick_spacing: f32,

    /// The color of the activity_line
    pub color: Rgb,

    /// The transparency of the activity line
    pub alpha: f32,

    /// The opengl representation of the activity levels
    /// Contains twice as many items because it's x,y
    pub metrics_opengl_vecs: Vec<f32>,

    /// The opengl representation of the activity levels
    /// Contains twice as many items because it's x,y
    pub marker_opengl_vecs: Vec<f32>,

    /// Missing values can be set to zero
    /// to show where the 1 task per core is
    pub missing_values_policy: MissingValuesPolicy<T>,
}

impl<T> TimeSeries<T>
where T: Num + Clone + Copy
{
    pub fn update(&mut self, input: (u64, T))
        where T: Num + Clone + Copy + PartialOrd + ToPrimitive + Bounded + FromPrimitive
    {
        let mut activity_time_length = self.metrics.len();
        if activity_time_length == 0 {
            self.metrics.push(input);
            // TODO: update_opengl_vecs(size);
            return;
        }
        let last_activity_time = self.metrics[activity_time_length - 1].0;
        if input.0 == last_activity_time {
            // The Vector is populated and has one active item at least which
            // we can work on, no need to rotate or do anything special
            match self.collision_policy {
                ValueCollisionPolicy::Increment => self.metrics[activity_time_length - 1] = (input.0, self.metrics[activity_time_length - 1].1 + input.1),
                ValueCollisionPolicy::Overwrite => self.metrics[activity_time_length - 1] = input,
                ValueCollisionPolicy::Decrement => self.metrics[activity_time_length - 1] = (input.0, self.metrics[activity_time_length - 1].1 - input.1),
                _ => self.metrics[activity_time_length - 1] = input,
            };
        }
    }
    fn update_now(&mut self, input: T)
        where T: Num + Clone + Copy + PartialOrd + ToPrimitive + Bounded + FromPrimitive
    {
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.update((now, input));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn it_rotates() {
        // The default includes an Increment policy
        let mut test = TimeSeries::default();
        // Initialize to 0,0
        test.update((0,0));
        assert_eq!(test.metrics, vec![(0,0)]);
        // Overwrite current entry
        test.update((0,1));
        assert_eq!(test.metrics, vec![(0,1)]);
        // Increment current entry
        test.update((0,1));
        assert_eq!(test.metrics, vec![(0,2)]);
    }
        /*let size = SizeInfo{
            width: 100f32,
            height: 100f32,
            cell_width: 1f32,
            cell_height: 1f32,
            padding_x: 0f32,
            padding_y: 0f32,
            dpr: 1f64
        };*/
}
/*
    /// `draw` sends the time series representation of the TimeSeries to OpenGL
    /// context, this shouldn't be mut
    fn draw(&self);
    /// `max` returns the max value in the TimeSeries
    fn max(&self, input: &Vec<(u64, Self::MetricType)>) -> Self::MetricType
        where Self::MetricType: Num + PartialOrd
    {
        let mut max_activity_value = Self::MetricType::zero();
        let activity_time_length = input.len();
        for idx in 0..activity_time_length {
            if input[idx].1 > max_activity_value {
                max_activity_value = input[idx].1;
            }
        }
        max_activity_value
    }
    fn update_opengl_vecs(size: SizeInfo) -> Vec<f32>{
        unimplemented!("XXX");
    }
    fn update(&mut self, &mut metrics: Vec<(u64, Self::MetricType)>, input: (u64, Self::MetricType),  collision_policy: ValueCollisionPolicy)
        where Self::MetricType: Num + Clone + Copy + PartialOrd + ToPrimitive + Bounded + FromPrimitive
    {
        let mut activity_time_length = metrics.len();
        if activity_time_length == 0 {
            metrics.push(input);
            // TODO: update_opengl_vecs(size);
        }
        let last_activity_time = metrics[activity_time_length - 1].0;
        if input.0 == last_activity_time {
            // The Vector is populated and has one active item at least which
            // we can work on, no need to rotate or do anything special
            match collision_policy {
                ValueCollisionPolicy::Increment => metrics[activity_time_length - 1] = (input.0, metrics[activity_time_length - 1].1 + input.1),
                ValueCollisionPolicy::Overwrite => metrics[activity_time_length - 1] = input,
                ValueCollisionPolicy::Decrement => metrics[activity_time_length - 1] = (input.0, metrics[activity_time_length - 1].1 - input.1),
                _ => metrics[activity_time_length - 1] = input,
            };
            return;
        }
        // Every time unit (currently second) is stored as an item in the array
        // Rotation may be needed due to inactivity or the array being filled
        self.rotate_activity_levels_vec(input.0);
        activity_time_length = metrics.len();
        if activity_time_length < self.max_activity_ticks {
            self.activity_levels.push(new_value);
        } else {
            self.activity_levels[activity_time_length - 1] = new_value;
        }
        // TODO: self.update_activity_opengl_vecs(size);
    }
    // fn from_prometheus(&mut self, &mut metrics: Vec<(u64, Self::MetricType)>, url: String)
    //     where Self::MetricType: Num + Clone + Copy + PartialOrd + ToPrimitive + Bounded + FromPrimitive
    // {
    // }

    // `init_opengl_context` provides a default initialization of OpengL
    // context. This function is called previous to sending the vector data.
    // This seems to be part of src/renderer/ mod tho...
    // fn init_opengl_context(&self);
}

/// `ActivityLevels` keep track of the activity per time tick
/// Currently this is a second as we use UNIX_EPOCH
#[derive(Debug, Clone)]
pub struct ActivityLevels<T>
where T: Num + Clone + Copy
{
    /// Capture events/characters per second
    /// Contains one entry per second
    pub activity_levels: Vec<T>,

    /// Last Activity Time
    pub last_activity_time: u64,

    /// Max activity ticks to show, ties to the activity_levels array
    /// it should cause it to throw away old items for newer records
    pub max_activity_ticks: usize,

    /// The color of the activity_line
    pub color: Rgb,

    /// The offset in which the activity line should be drawn
    pub x_offset: f32,

    /// The width of the activity chart/histogram
    pub width: f32,

    /// The opengl representation of the activity levels
    /// Contains twice as many items because it's x,y
    pub activity_opengl_vecs: Vec<f32>,

    /// The height of the activity line region
    pub activity_line_height: f32,

    /// The spacing between the activity line segments, could be renamed to line length
    pub activity_tick_spacing: f32,

    /// The transparency of the activity line
    pub alpha: f32,

    /// Useful for records that do not increment but rather are a fixed
    /// or absolute value recorded at a given time
    pub overwrite_last_entry: bool,

    /// A marker line to indicate a reference point, for example for load
    /// to show where the 1 loadavg is, or to show disk capacity
    pub marker_line: Option<T>,

    /// The opengl representation of the activity levels
    /// Contains twice as many items because it's x,y
    pub marker_line_vecs: Vec<f32>,

    /// Missing values can be set to zero
    /// to show where the 1 task per core is
    pub missing_values_policy: MissingValuesPolicy<T>,
}

impl<T> Default for ActivityLevels<T>
where T: Num + Clone + Copy
{
    /// `default` provides 5mins activity lines with red color
    /// The vector of activity_levels per second is created with reserved capacity,
    /// just to avoid needed to reallocate the vector in memory the first 5mins.
    fn default() -> ActivityLevels<T>{
        let activity_time = std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let activity_vector_capacity = 300usize; // 300 seconds (5mins)
        ActivityLevels{
            last_activity_time: activity_time,
            activity_levels: Vec::<T>::with_capacity(activity_vector_capacity), // XXX: Maybe use vec![0; 300]; to pre-fill with zeros?
            max_activity_ticks: activity_vector_capacity,
            color: Rgb { // By default red
                r: 183,
                g: 28,
                b: 28,
            },
            x_offset: 600f32,
            width: 200f32,
            activity_opengl_vecs: Vec::<f32>::with_capacity(activity_vector_capacity * 2),
            marker_line_vecs: vec![0f32; 16],
            activity_line_height: 25f32,
            activity_tick_spacing: 5f32,
            alpha: 1f32,
            overwrite_last_entry: false,
            marker_line: None,
            missing_values_policy: MissingValuesPolicy::Zero,
        }
    }
}

impl<T> ActivityLevels<T>
where T: Num + Clone + Copy
{
    /// `with_color` Changes the color of the activity line
    pub fn with_color(mut self, color: Rgb) -> ActivityLevels<T> {
        self.color = color;
        self
    }

    /// `with_x_offset` Changes the offset of the activity level drawing location
    pub fn with_x_offset(mut self, new_offset: f32) -> ActivityLevels<T> {
        self.x_offset = new_offset;
        self
    }

    /// `with_width` Changes the width of the activity level drawing location
    pub fn with_width(mut self, width: f32) -> ActivityLevels<T> {
        self.width = width;
        self
    }

    /// `with_alpha` Changes the transparency of the activity level
    pub fn with_alpha(mut self, alpha: f32) -> ActivityLevels<T> {
        self.alpha = alpha;
        self
    }

    /// `with_overwrite_last_entry` overwrite instead of increment current time
    /// entry
    pub fn with_overwrite_last_entry(mut self, value: bool) -> ActivityLevels<T> {
        self.overwrite_last_entry = value;
        self
    }
     
    /// `with_marker_line` initializes the marker line into a Some
    pub fn with_marker_line(mut self, value: T) -> ActivityLevels<T> {
        self.marker_line = Some(value);
        self
    }

    /// `with_missing_values_policy` receives a String and returns 
    /// a MissingValuesPolicy, TODO: the "Fixed" value is not implemented.
    pub fn with_missing_values_policy(mut self, policy_type: String) -> ActivityLevels<T> {
        self.missing_values_policy = match policy_type.as_ref() {
            "zero"  => MissingValuesPolicy::Zero,
            "one"   => MissingValuesPolicy::One,
            "min"   => MissingValuesPolicy::Min,
            "max"   => MissingValuesPolicy::Max,
            "last"  => MissingValuesPolicy::Last,
            "avg"   => MissingValuesPolicy::Avg,
            "first" => MissingValuesPolicy::First,
            _ => {
                // Expect a number and transform it into T, this is the "Fixed"
                // enum variant, basically any string incoming
                // For now, let's set it to 1
                MissingValuesPolicy::First
            },
        };
        self
    }

    pub fn get_missing_values_fill(&self) -> T
    where T: Num + Clone + Copy + PartialOrd + Bounded + FromPrimitive
    {
        // Recalculating seems to be necessary because we are constantly
        // moving items out of the Vec<> so our cache can easily get out of
        // sync
        let mut max_activity_value = T::zero();
        let mut min_activity_value = T::max_value();
        let mut sum_activity_values = T::zero();
        let activity_time_length = self.activity_levels.len();
        for idx in 0..activity_time_length {
            if self.activity_levels[idx] > max_activity_value {
                max_activity_value = self.activity_levels[idx];
            }
            if self.activity_levels[idx] < min_activity_value {
                min_activity_value = self.activity_levels[idx];
            }
            sum_activity_values = sum_activity_values + self.activity_levels[idx];
        }
        // XXX: If the values are being shifted, these min/max will be
        // deceiving, on the other hand, it would just be deceiving for the
        // first draw after long period of inactivity, which also shows
        // visually how things are changing.
        match self.missing_values_policy {
            MissingValuesPolicy::Zero => T::zero(),
            MissingValuesPolicy::One => T::one(),
            MissingValuesPolicy::Min => min_activity_value,
            MissingValuesPolicy::Max => max_activity_value,
            MissingValuesPolicy::Last => self.activity_levels[activity_time_length - 1],
            MissingValuesPolicy::First => self.activity_levels[0],
            MissingValuesPolicy::Avg => sum_activity_values / num_traits::FromPrimitive::from_usize(activity_time_length).unwrap(),
            MissingValuesPolicy::Fixed(val) => val,
        }
    }

    /// `rotate_activity_levels_vec` when either we run out of our vector
    /// capacity in the vector or when the terminal has been inactive enough
    /// that in needs the vector to be rotated.
    pub fn rotate_activity_levels_vec(&mut self, now: u64)
    where T: Num + Clone + Copy + PartialOrd + Bounded + FromPrimitive
    {
        let activity_time_length = self.activity_levels.len();
        let mut inactive_time = (now - self.last_activity_time) as usize;
        if inactive_time > self.max_activity_ticks {
            inactive_time = self.max_activity_ticks;
        }
        let missing_timeslots_fill = self.get_missing_values_fill();
        if inactive_time + activity_time_length > self.max_activity_ticks {
            let shift_left_times = inactive_time + activity_time_length - self.max_activity_ticks;
            for idx in 0 .. activity_time_length - shift_left_times {
                self.activity_levels[idx] = self.activity_levels[idx + shift_left_times]
            }
            for idx in activity_time_length - shift_left_times .. activity_time_length {
                self.activity_levels[idx] = missing_timeslots_fill;
            }
        } else {
            for _ in 0..inactive_time - 1 {
                self.activity_levels.push(missing_timeslots_fill);
            }
        }
    }

    /// `update_activity_level` Ensures time slots are filled with 0s for
    /// inactivity and increments the current epoch activity_level slot by an
    /// new_value, it uses the size to calculate the position from the
    /// bottom in which to display the activity levels
    pub fn update_activity_level(&mut self,
                                 size: SizeInfo,
                                 new_value: T
                                 )
    where T: Num + Clone + Copy + PartialOrd + ToPrimitive + Bounded + FromPrimitive
    {
        // XXX: Right now set to "as_secs", but could be used for other time units easily
        let mut activity_time_length = self.activity_levels.len();
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if activity_time_length == 0 {
            // The vector is empty, no need to rotate or do anything special
            self.activity_levels.push(new_value);
            self.last_activity_time = now;
            self.update_activity_opengl_vecs(size);
            return;
        }
        if now == self.last_activity_time {
            // The Vector is populated and has one active item at least which
            // we can work on, no need to rotate or do anything special
            if self.overwrite_last_entry {
                self.activity_levels[activity_time_length - 1] = new_value;
            } else {
                self.activity_levels[activity_time_length - 1] = self.activity_levels[activity_time_length - 1] + new_value;
            }
            self.update_activity_opengl_vecs(size);
            return;
        }
        // Every time unit (currently second) is stored as an item in the array
        // Rotation may be needed due to inactivity or the array being filled
        self.rotate_activity_levels_vec(now);
        activity_time_length = self.activity_levels.len();
        if activity_time_length < self.max_activity_ticks {
            self.activity_levels.push(new_value);
        } else {
            self.activity_levels[activity_time_length - 1] = new_value;
        }
        self.last_activity_time = now;
        self.update_activity_opengl_vecs(size);
    }
    
    /// `scale_y_to_size` Scales the value to the current display boundary
    pub fn scale_y_to_size(&self, size: SizeInfo, input_value: T, max_activity_value: T) -> f32
    where T: Num + ToPrimitive
    {
        let center_y = size.height / 2.;
        let y = size.height -
                2. * size.padding_y -
                ( self.activity_line_height *
                  num_traits::ToPrimitive::to_f32(&input_value).unwrap() /
                  num_traits::ToPrimitive::to_f32(&max_activity_value).unwrap()
                );
        -(y - center_y) / center_y
    }

    /// `scale_x_to_size` Scales the value to the current display boundary
    pub fn scale_x_to_size(&self, size: SizeInfo, input_value: f32) -> f32
    where T: Num + ToPrimitive
    {
        let center_x = size.width / 2.;
        let x = size.padding_x + self.x_offset + input_value;
        (x - center_x) / center_x
    }


    /// `update_marker_line_vecs` Scales the Marker Line to the current size of
    /// the displayed points
    pub fn update_marker_line_vecs(&mut self, size: SizeInfo, max_activity_value: T, marker_line_position: T)
    where T: Num + PartialOrd + ToPrimitive + Bounded + FromPrimitive
    {
        // TODO: Add marker_line color
        // Draw a marker at a fixed position for reference: |>---------<|
        // The vertexes of the above marker idea can be represented as
        // connecting lines for these coordinates:
        // x2,y2                               x6,y2
        //       x1,y1                   x5,y1
        // x2,y3                               x6,y3
        // |-   10%   -|-     80%     -|-   10%   -|

        // Calculate X, the triangle width is 10% of the available draw space
        let x1 = self.scale_x_to_size(size, self.width / 10f32);
        let x2 = self.scale_x_to_size(size, 0f32);
        let x5 = self.scale_x_to_size(size, self.width - self.width / 10f32);
        let x6 = self.scale_x_to_size(size, self.width);

        // Calculate X, the triangle height is 10% of the available draw space
        let y1 = self.scale_y_to_size(size,
                                      marker_line_position,
                                      max_activity_value); // = y4,y5,y8
        let y2 = y1 - self.scale_y_to_size(size,max_activity_value,max_activity_value) / 100f32; // = y7
        let y3 = y1 + self.scale_y_to_size(size,max_activity_value,max_activity_value) / 100f32; // = y7

        // Left triangle |>
        self.marker_line_vecs[0] = x1;
        self.marker_line_vecs[1] = y1;
        self.marker_line_vecs[2] = x2;
        self.marker_line_vecs[3] = y2;
        self.marker_line_vecs[4] = x2;
        self.marker_line_vecs[5] = y3;

        // Line from left triangle to right triangle ---
        self.marker_line_vecs[6] = x1;
        self.marker_line_vecs[7] = y1;
        self.marker_line_vecs[8] = x5;
        self.marker_line_vecs[9] = y1;

        // Right triangle <|
        self.marker_line_vecs[10] = x6;
        self.marker_line_vecs[11] = y3;
        self.marker_line_vecs[12] = x6;
        self.marker_line_vecs[13] = y2;

        // And loop back to x5,y5
        self.marker_line_vecs[14] = x5;
        self.marker_line_vecs[15] = y1;

    }

    /// `update_opengl_vecs` Represents the activity levels values in a
    /// drawable vector for opengl
    pub fn update_activity_opengl_vecs(&mut self, size: SizeInfo)
    where T: Num + PartialOrd + ToPrimitive + Bounded + FromPrimitive
    {
        // Get the maximum value
        let mut max_activity_value = T::zero();
        for idx in 0..self.activity_levels.len() {
            if self.activity_levels[idx] > max_activity_value {
                max_activity_value = self.activity_levels[idx];
            }
        }
        if let Some(marker_line_value) = self.marker_line {
            if marker_line_value > max_activity_value {
                max_activity_value = marker_line_value;
            }
        }
        // Get the opengl representation of the vector
        let opengl_vecs_len = self.activity_opengl_vecs.len();
        // Calculate the tick spacing
        let tick_spacing = if self.marker_line.is_some() {
            // Subtract 20% of the horizonal draw space that is allocated for
            // the Marker Line
            self.width * 0.2 / self.max_activity_ticks as f32
        } else {
            self.width / self.max_activity_ticks as f32
        };
        for idx in 0..self.activity_levels.len() {
            let mut x_value = idx as f32 * tick_spacing;
            // If there is a Marker Line, it takes 10% of the initial horizontal space
            if self.marker_line.is_some() {
                x_value += self.width * 0.1;
            }
            let scaled_x = self.scale_x_to_size(size, x_value);
            let scaled_y = self.scale_y_to_size(size, self.activity_levels[idx], max_activity_value);
            // Adding twice to a vec, could this be made into one operation? Is this slow?
            // need to transform activity line values from varying levels into scaled [-1, 1]
            if (idx + 1) * 2 > opengl_vecs_len {
                self.activity_opengl_vecs.push(scaled_x);
                self.activity_opengl_vecs.push(scaled_y);
            } else {
                self.activity_opengl_vecs[idx * 2] = scaled_x;
                self.activity_opengl_vecs[idx * 2 + 1] = scaled_y;
            }
        }
        if let Some(marker_line_value) = self.marker_line {
            self.update_marker_line_vecs(size, max_activity_value, marker_line_value);
        }
    }
}

pub struct PrometheusMetric {
    name: String,
    url: String,
}

impl PrometheusMetric {
    pub fn load() -> Future {
        rt::run(rt::lazy(|| {
            let client = Client::new();
            let uri = format!("http://localhost:9090/api/v1/query?{}",query)
                .parse()
                .unwrap();
            client
                .get(uri)
                .and_then(|res| {
                    println!("Response: {}", res.status());
                    res
                        .into_body()
                        // Body is a stream, so as each chunk arrives...
                        .for_each(|chunk| {
                            io::stdout()
                                .write_all(&chunk)
                                .map_err(|e| {
                                    panic!("example expects stdout is open, error={}", e)
                                })
                        })
                })
                .map_err(|err| {
                    println!("Error: {}", err);
                })
        }));
    }
    pub fn new(name: String, query: String) -> PrometheusMetric {
        PrometheusMetric {
            url: format!("http://localhost:9090/api/v1/query?{}",query),
            name,
        }
    }
}

pub struct LoadAvg {
    /// From https://docs.rs/procinfo/0.4.2/procinfo/struct.LoadAvg.html
    /// Load average over the last minute.
    load_avg_1_min: ActivityLevels<f32>,

    /// Load average of the last 5 minutes.
    load_avg_5_min: ActivityLevels<f32>,

    /// Load average of the last 10 minutes
    load_avg_10_min: ActivityLevels<f32>,

    // These metrics are not that interesting to graph:
    // the number of currently runnable kernel scheduling entities (processes, threads).
    // tasks_runnable: ActivityLevels<u32>,
    // the number of kernel scheduling entities that currently exist on the system.
    // tasks_total: ActivityLevels<u32>,

    /// If no metrics were collected for some time, fill them with the last 
    /// known value
    missing_values_policy: MissingValuesPolicy<f32>,

    /// A marker line to indicate a reference point, for example for load
    /// to show where the 1 loadavg is, or to show disk capacity
    pub marker_line: Option<f32>,

    /// The opengl representation of the activity levels
    /// Contains twice as many items because it's x,y
    pub marker_line_vecs: Vec<f32>,
}

impl Default for LoadAvg{
    fn default() -> LoadAvg {
        LoadAvg{
            load_avg_1_min: ActivityLevels::default()
                .with_color(Rgb{r:93,g:23,b:106})
                .with_width(50f32)
                .with_alpha(0.9)
                .with_missing_values_policy("last".to_string())
                .with_marker_line(1f32)
                .with_overwrite_last_entry(true)
                .with_x_offset(1010f32),
            load_avg_5_min: ActivityLevels::default()
                .with_color(Rgb{r:146,g:75,b:158})
                .with_width(30f32)
                .with_alpha(0.6)
                .with_missing_values_policy("last".to_string())
                .with_marker_line(1f32)
                .with_overwrite_last_entry(true)
                .with_x_offset(1070f32),
            load_avg_10_min: ActivityLevels::default()
                .with_color(Rgb{r:202,g:127,b:213})
                .with_width(20f32)
                .with_alpha(0.3)
                .with_missing_values_policy("last".to_string())
                .with_marker_line(1f32) // Set a reference point at load 1
                .with_overwrite_last_entry(true)
                .with_x_offset(1110f32),
            //tasks_runnable: ActivityLevels::default()
            //    .with_color(Rgb{r:0,g:172,b:193})
            //    .with_width(50f32)
            //    .with_missing_values_policy("last".to_string())
            //    .with_overwrite_last_entry(true)
            //    .with_x_offset(1140f32),
            //tasks_total: ActivityLevels::default()
            //    .with_color(Rgb{r:27,g:160,b:71})
            //    .with_width(50f32)
            //    .with_missing_values_policy("last".to_string())
            //    .with_overwrite_last_entry(true)
            //    .with_x_offset(1190f32),
            missing_values_policy: MissingValuesPolicy::Last,
            marker_line: Some(1f32),
            marker_line_vecs: vec![0f32; 16],
        }
    }
}

impl TimeSeries for LoadAvg {
    type MetricType = f32;
    fn draw(&self) {
    }
    fn max(&self, _input: &Vec<Self::MetricType>) -> Self::MetricType 
        where Self::MetricType: Num + PartialOrd
    {
        let mut max_activity_value = TimeSeries::max(self, &self.load_avg_1_min.activity_levels);
        let max_5_min = TimeSeries::max(self, &self.load_avg_5_min.activity_levels);
        if max_activity_value < max_5_min {
            max_activity_value = max_5_min;
        }
        let max_10_min = TimeSeries::max(self, &self.load_avg_10_min.activity_levels);
        if max_activity_value < max_10_min {
            max_activity_value = max_10_min;
        }
        max_activity_value
    }

}*/

//impl TimeSeries for ActivityLevels {
//    fn draw()
//}
