//! Rendering time meter.
//!
//! Used to track rendering times and provide moving averages.
//!
//! # Examples
//!
//! ```rust
//! // create a meter
//! let mut meter = alacritty_terminal::meter::Meter::new();
//!
//! // Sample something.
//! {
//!     let _sampler = meter.sampler();
//! }
//!
//! // Get the moving average. The meter tracks a fixed number of samples, and
//! // the average won't mean much until it's filled up at least once.
//! println!("Average time: {}", meter.average());
//! ```

use std::time::{Duration, Instant};

const NUM_SAMPLES: usize = 30;

/// The meter.
#[derive(Default)]
pub struct Meter {
    /// Track last 60 timestamps.
    times: [f64; NUM_SAMPLES],

    /// Average sample time in microseconds.
    avg: f64,

    /// Index of next time to update.
    index: usize,
}

/// Sampler.
///
/// Samplers record how long they are "alive" when being consumed by the `Meter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    /// The timestamp sample should use as a base.
    base: Instant,
}

impl Sample {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_base(base: Instant) -> Self {
        Self { base }
    }

    #[inline]
    fn alive_duration(&self) -> Duration {
        self.base.elapsed()
    }
}

impl Default for Sample {
    fn default() -> Self {
        Self { base: Instant::now() }
    }
}

impl Meter {
    /// Create a meter.
    pub fn new() -> Meter {
        Default::default()
    }

    /// Get a sample.
    pub fn sample(&self) -> Sample {
        Sample::new()
    }

    /// Get the current average sample duration in microseconds.
    pub fn average(&self) -> f64 {
        self.avg
    }

    /// Add a sample.
    pub fn add_sample(&mut self, sample: Sample) {
        let mut usec = 0f64;

        let sample = sample.alive_duration();

        usec += f64::from(sample.subsec_nanos()) / 1e3;
        usec += (sample.as_secs() as f64) * 1e6;

        let prev = self.times[self.index];
        self.times[self.index] = usec;
        self.avg -= prev / NUM_SAMPLES as f64;
        self.avg += usec / NUM_SAMPLES as f64;
        self.index = (self.index + 1) % NUM_SAMPLES;
    }
}
