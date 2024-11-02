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

const NUM_SAMPLES: usize = 10;

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
/// Samplers record how long they are "alive" for and update the meter on drop..
pub struct Sampler<'a> {
    /// Reference to meter that created the sampler.
    meter: &'a mut Meter,

    /// When the sampler was created.
    created_at: Instant,
}

impl<'a> Sampler<'a> {
    fn new(meter: &'a mut Meter) -> Sampler<'a> {
        Sampler { meter, created_at: Instant::now() }
    }

    #[inline]
    fn alive_duration(&self) -> Duration {
        self.created_at.elapsed()
    }
}

impl Drop for Sampler<'_> {
    fn drop(&mut self) {
        self.meter.add_sample(self.alive_duration());
    }
}

impl Meter {
    /// Get a sampler.
    pub fn sampler(&mut self) -> Sampler<'_> {
        Sampler::new(self)
    }

    /// Get the current average sample duration in microseconds.
    pub fn average(&self) -> f64 {
        self.avg
    }

    /// Add a sample.
    ///
    /// Used by Sampler::drop.
    fn add_sample(&mut self, sample: Duration) {
        let mut usec = 0f64;

        usec += f64::from(sample.subsec_nanos()) / 1e3;
        usec += (sample.as_secs() as f64) * 1e6;

        let prev = self.times[self.index];
        self.times[self.index] = usec;
        self.avg -= prev / NUM_SAMPLES as f64;
        self.avg += usec / NUM_SAMPLES as f64;
        self.index = (self.index + 1) % NUM_SAMPLES;
    }
}
