//! Exports the TimeSeries class

// TODO:
// - Move to the config.yaml
// -- The yaml should drive an array of activity dashboards
// -- The dashboards should be toggable, some key combination
// -- When activated on toggle it could blur a portion of the screen
// -- derive builder
// -- Use prometheus queries instead of our own aggregation/etc.
// -- The vectors should be circular to avoid constantly rotating

extern crate futures;
extern crate hyper;
extern crate num_traits;
extern crate serde_json;
extern crate tokio_core;
use crate::term::color::Rgb;
use crate::term::SizeInfo;
use num_traits::*;
use std::time::UNIX_EPOCH;

use futures::{Future, Stream};
use hyper::Client;
use serde_json::Value;
use std::io;
use tokio_core::reactor::Core;

/// `MissingValuesPolicy` provides several ways to deal with missing values
/// when drawing the Metric
#[derive(Debug, Clone)]
pub enum MissingValuesPolicy<T>
where
    T: Num + Clone + Copy,
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
where
    T: Num + Clone + Copy,
{
    fn default() -> MissingValuesPolicy<T> {
        MissingValuesPolicy::Zero
    }
}

/// `ValueCollisionPolicy` handles collisions when several values are collected
/// for the same time unit, allowing for overwriting, incrementing, etc.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct TimeSeriesStats<T>
where
    T: Num + Clone + Copy,
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

impl<T> Default for TimeSeriesStats<T>
where
    T: Num + Clone + Copy,
{
    fn default() -> TimeSeriesStats<T> {
        TimeSeriesStats {
            max: T::zero(),
            min: T::zero(),
            avg: T::zero(),
            first: T::zero(),
            last: T::zero(),
            count: 0usize,
            sum: T::zero(),
            is_dirty: false,
        }
    }
}
/// `TimeSeries` contains a vector of tuple (epoch, value)
pub struct TimeSeries<T>
where
    T: Num + Clone + Copy,
{
    /// Capture events through time
    /// Contains one entry per time unit
    pub metrics: Vec<(u64, Option<T>)>,

    /// Number of items to store in our metrics vec
    pub metrics_capacity: usize,

    /// Stats for the TimeSeries
    pub metric_stats: TimeSeriesStats<T>,

    /// Useful for records that do not increment but rather are a fixed
    /// or absolute value recorded at a given time
    pub collision_policy: ValueCollisionPolicy,

    /// Missing values can be set to zero
    /// to show where the 1 task per core is
    pub missing_values_policy: MissingValuesPolicy<T>,
}

pub struct TimeSeriesChart<T>
where
    T: Num + Clone + Copy,
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
}

impl<T> Default for TimeSeries<T>
where
    T: Num + Clone + Copy,
{
    /// `new` returns the default
    fn default() -> TimeSeries<T> {
        // This leads to 5 mins of metrics to show by default.
        let default_capacity = 300usize;
        TimeSeries {
            metrics_capacity: default_capacity,
            metrics: Vec::with_capacity(default_capacity),
            metric_stats: TimeSeriesStats::default(),
            collision_policy: ValueCollisionPolicy::default(),
            missing_values_policy: MissingValuesPolicy::default(),
        }
    }
}
impl<T> TimeSeries<T>
where
    T: Num + Clone + Copy,
{
    /// `with_capacity` builder changes the amount of metrics in the vec
    pub fn with_capacity(self, n: usize) -> TimeSeries<T> {
        let mut new_self = self;
        new_self.metrics = Vec::with_capacity(n);
        new_self.metrics_capacity = n;
        new_self
    }

    /// `with_missing_values_policy` receives a String and returns
    /// a MissingValuesPolicy, TODO: the "Fixed" value is not implemented.
    pub fn with_missing_values_policy(mut self, policy_type: String) -> TimeSeries<T> {
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

    /// `calculate_stats` Checks if stats need to be updated for the current
    /// metrics
    pub fn calculate_stats(&mut self)
    where
        T: Num + Clone + Copy + PartialOrd + Bounded + FromPrimitive,
    {
        // Recalculating seems to be necessary because we are constantly
        // moving items out of the Vec<> so our cache can easily get out of
        // sync
        let mut max_activity_value = T::zero();
        let mut min_activity_value = T::max_value();
        let mut sum_activity_values = T::zero();
        let mut filled_metrics = 0usize;
        for idx in 0..self.metrics.len() {
            if let Some(metric) = self.metrics[idx].1 {
                if metric > max_activity_value {
                    max_activity_value = metric;
                }
                if metric < min_activity_value {
                    min_activity_value = metric;
                }
                sum_activity_values = sum_activity_values + metric;
                filled_metrics += 1;
            }
        }
        self.metric_stats.max = max_activity_value;
        self.metric_stats.min = min_activity_value;
        self.metric_stats.sum = sum_activity_values;
        self.metric_stats.avg =
            sum_activity_values / num_traits::FromPrimitive::from_usize(filled_metrics).unwrap();
    }

    /// `get_missing_values_fill` uses the MissingValuesPolicy to decide
    /// which value to place on empty metric timeslots when drawing
    pub fn get_missing_values_fill(&mut self) -> T
    where
        T: Num + Clone + Copy + PartialOrd + Bounded + FromPrimitive,
    {
        // XXX: If the values are being shifted, these min/max will be
        // deceiving, on the other hand, it would just be deceiving for the
        // first draw after long period of inactivity, which also shows
        // visually how things are changing.
        self.calculate_stats();
        match self.missing_values_policy {
            MissingValuesPolicy::Zero => T::zero(),
            MissingValuesPolicy::One => T::one(),
            MissingValuesPolicy::Min => self.metric_stats.min,
            MissingValuesPolicy::Max => self.metric_stats.max,
            MissingValuesPolicy::Last => {
                T::zero()
                // TODO: iterate from front to back to get the last filled stat:
                // self.metrics[self.metrics.len() - 1].1,
            },
            MissingValuesPolicy::First => {
                T::zero()
                // TODO: iterate from back to front to get the first filled stat:
                // self.metrics[0].1,
            },
            MissingValuesPolicy::Avg => self.metric_stats.avg,
            MissingValuesPolicy::Fixed(val) => val,
        }
    }

    /// `rotate_metrics` when we run out of our vector
    /// capacity or when the terminal has been inactive enough
    /// that in needs the vector to be rotated.
    pub fn rotate_metrics(&mut self, epoch: u64)
    where
        T: Num + Clone + Copy + PartialOrd + Bounded + FromPrimitive,
    {
        let metrics_length = self.metrics.len();
        if metrics_length == 0 {
            return;
        }
        let max_metrics_epoch = self.metrics[metrics_length - 1].0;
        if max_metrics_epoch == epoch {
            return;
        }
        let inactive_time = (epoch - self.metrics[metrics_length - 1].0) as usize;
        if inactive_time > self.metrics_capacity {
            // The whole vector is outdated, fill the vector as empty
            for idx in 0..self.metrics_capacity {
                let fill_epoch = epoch - self.metrics_capacity as u64 + idx as u64 + 1;
                if idx < metrics_length {
                    self.metrics[idx] = (fill_epoch, None);
                } else {
                    self.metrics.push((fill_epoch, None));
                }
            }
        } else if inactive_time + metrics_length > self.metrics_capacity {
            let shift_left_times = inactive_time + metrics_length - self.metrics_capacity;
            for idx in 0..metrics_length - shift_left_times {
                self.metrics[idx] = self.metrics[idx + shift_left_times]
            }
            let mut fill_epoch = self.metrics[metrics_length - shift_left_times].0;
            for idx in metrics_length - shift_left_times..metrics_length {
                fill_epoch += 1;
                self.metrics[idx] = (fill_epoch, None);
            }
        } else if inactive_time > 1 {
            // Fill the inactive time as None
            for idx in 0..inactive_time - 1 {
                self.metrics.push((max_metrics_epoch + 1u64 + idx as u64, None));
            }
        }
    }

    /// `resolve_metric_collision` ensures the policy for colliding values is
    /// applied.
    pub fn resolve_metric_collision(&self, existing: T, new: T) -> T {
        match self.collision_policy {
            ValueCollisionPolicy::Increment => existing + new,
            ValueCollisionPolicy::Overwrite => new,
            ValueCollisionPolicy::Decrement => existing - new,
            ValueCollisionPolicy::Ignore => existing,
        }
    }

    /// `update` Adds an input metric on a specif epoch to the metrics vector
    pub fn update(&mut self, input: (u64, T))
    where
        T: Num + Clone + Copy + PartialOrd + ToPrimitive + Bounded + FromPrimitive,
    {
        // Rotation might be needed to discard old values or clear inactivity
        self.rotate_metrics(input.0);
        let metrics_length = self.metrics.len();
        if metrics_length == 0 {
            // The vec is empty, just push the input.
            self.metrics.push((input.0, Some(input.1)));
        } else if input.0 == self.metrics[metrics_length - 1].0 {
            // The last metric epoch and the new metric are the same
            // Figure out wether to overwrite or increment metric
            let last_metric_value = self.metrics[metrics_length - 1].1;
            if let Some(metric) = last_metric_value {
                let resolved_metric = self.resolve_metric_collision(metric, input.1);
                self.metrics[metrics_length - 1] = (input.0, Some(resolved_metric));
            } else {
                // Technically this should never happen, but maybe loading from
                // Prometheus could lead into empty metrics at the front
                self.metrics[metrics_length - 1] = (input.0, Some(input.1));
            }
        } else if metrics_length < self.metrics_capacity {
            // There is enough space to push to the vector.
            self.metrics.push((input.0, Some(input.1)));
        } else {
            // There is not enough space to push to the vector.
            self.metrics[metrics_length - 1] = (input.0, Some(input.1));
        }
        // TODO: self.update_activity_opengl_vecs(size);
    }

    fn update_current_epoch(&mut self, input: T)
    where
        T: Num + Clone + Copy + PartialOrd + ToPrimitive + Bounded + FromPrimitive,
    {
        let now = std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        self.update((now, input));
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_rotates() {
        let mut test = TimeSeries::default().with_capacity(5);
        test.update((0, 0));
        assert_eq!(test.metrics, vec![(0, Some(0))]);
        test.rotate_metrics(0);
        assert_eq!(test.metrics, vec![(0, Some(0))]);
        // No need to rotate, the next push should do it
        test.rotate_metrics(1);
        assert_eq!(test.metrics, vec![(0, Some(0))]);
        test.update((1, 1));
        assert_eq!(test.metrics, vec![(0, Some(0)), (1, Some(1))]);
        test.rotate_metrics(3);
        assert_eq!(test.metrics, vec![(0, Some(0)), (1, Some(1)), (2, None)]);
        test.rotate_metrics(10);
        assert_eq!(test.metrics, vec![(6, None), (7, None), (8, None), (9, None), (10, None)]);
        test.update((10, 10));
        assert_eq!(test.metrics, vec![(6, None), (7, None), (8, None), (9, None), (10, Some(10))]);
        let mut test = TimeSeries::default().with_capacity(5);
        test.update((100, 0));
        test.update((100, 1));
        test.update((101, 1));
        test.update((103, 3));
        assert_eq!(test.metrics, vec![(100, Some(1)), (101, Some(1)), (102, None), (103, Some(3))]);
        test.rotate_metrics(105);
        assert_eq!(test.metrics, vec![(101, Some(1)), (102, None), (103, Some(3)), (104, None),]);
        test.update((105, 5));
        assert_eq!(test.metrics, vec![
            (101, Some(1)),
            (102, None),
            (103, Some(3)),
            (104, None),
            (105, Some(5))
        ]);
    }
    #[test]
    fn it_updates() {
        // The default includes an Increment policy
        let mut test = TimeSeries::default().with_capacity(5);
        // Initialize to 0,0
        test.update((1000, 0));
        assert_eq!(test.metrics, vec![(1000, Some(0))]);
        // Overwrite current entry
        test.update((1000, 1));
        assert_eq!(test.metrics, vec![(1000, Some(1))]);
        // Increment current entry
        test.update((1000, 1));
        assert_eq!(test.metrics, vec![(1000, Some(2))]);
        test.update((1001, 1));
        assert_eq!(test.metrics, vec![(1000, Some(2)), (1001, Some(1))]);
        test.update((1003, 3));
        assert_eq!(test.metrics, vec![
            (1000, Some(2)),
            (1001, Some(1)),
            (1002, None),
            (1003, Some(3))
        ]);
        test.update((1005, 5));
        assert_eq!(test.metrics, vec![
            (1001, Some(1)),
            (1002, None),
            (1003, Some(3)),
            (1004, None),
            (1005, Some(5))
        ]);
        test.update((1025, 25));
        assert_eq!(test.metrics, vec![
            (1021, None),
            (1022, None),
            (1023, None),
            (1024, None),
            (1025, Some(25))
        ]);
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
        test_zero.update((0, 9));
        test_zero.update((2, 1));
        test_one.update((0, 9));
        test_one.update((2, 1));
        test_min.update((0, 9));
        test_min.update((2, 1));
        test_max.update((0, 9));
        test_max.update((2, 1));
        test_last.update((0, 9));
        test_last.update((2, 1));
        test_first.update((0, 9));
        test_first.update((2, 1));
        test_avg.update((0, 9));
        test_avg.update((2, 1));
        assert_eq!(test_zero.get_missing_values_fill(), 0);
        assert_eq!(test_one.get_missing_values_fill(), 1);
        assert_eq!(test_min.get_missing_values_fill(), 1);
        assert_eq!(test_max.get_missing_values_fill(), 9);
        // assert_eq!(test_last.get_missing_values_fill(), 1); // TODO: FIX
        // assert_eq!(test_first.get_missing_values_fill(), 9); // TODO: FIX
        assert_eq!(test_avg.get_missing_values_fill(), 5);
        // TODO: add Fixed test
    }
    // let size = SizeInfo{
    // width: 100f32,
    // height: 100f32,
    // cell_width: 1f32,
    // cell_height: 1f32,
    // padding_x: 0f32,
    // padding_y: 0f32,
    // dpr: 1f64
    // };
}
