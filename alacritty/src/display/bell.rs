use std::time::{Duration, Instant};

use crate::config::bell::{BellAnimation, BellConfig};

pub struct VisualBell {
    /// Visual bell animation.
    animation: BellAnimation,

    /// Visual bell duration.
    duration: Duration,

    /// The last time the visual bell rang, if at all.
    start_time: Option<Instant>,
}

impl VisualBell {
    /// Ring the visual bell, and return its intensity.
    pub fn ring(&mut self) -> f64 {
        let now = Instant::now();
        self.start_time = Some(now);
        self.intensity_at_instant(now)
    }

    /// Get the currently intensity of the visual bell. The bell's intensity
    /// ramps down from 1.0 to 0.0 at a rate determined by the bell's duration.
    pub fn intensity(&self) -> f64 {
        self.intensity_at_instant(Instant::now())
    }

    /// Check whether or not the visual bell has completed "ringing".
    pub fn completed(&mut self) -> bool {
        match self.start_time {
            Some(earlier) => {
                if Instant::now().duration_since(earlier) >= self.duration {
                    self.start_time = None;
                }
                false
            },
            None => true,
        }
    }

    /// Get the intensity of the visual bell at a particular instant. The bell's
    /// intensity ramps down from 1.0 to 0.0 at a rate determined by the bell's
    /// duration.
    pub fn intensity_at_instant(&self, instant: Instant) -> f64 {
        // If `duration` is zero, then the VisualBell is disabled; therefore,
        // its `intensity` is zero.
        if self.duration == Duration::from_secs(0) {
            return 0.0;
        }

        match self.start_time {
            // Similarly, if `start_time` is `None`, then the VisualBell has not
            // been "rung"; therefore, its `intensity` is zero.
            None => 0.0,

            Some(earlier) => {
                // Finally, if the `instant` at which we wish to compute the
                // VisualBell's `intensity` occurred before the VisualBell was
                // "rung", then its `intensity` is also zero.
                if instant < earlier {
                    return 0.0;
                }

                let elapsed = instant.duration_since(earlier);
                let elapsed_f =
                    elapsed.as_secs() as f64 + f64::from(elapsed.subsec_nanos()) / 1e9f64;
                let duration_f = self.duration.as_secs() as f64
                    + f64::from(self.duration.subsec_nanos()) / 1e9f64;

                // Otherwise, we compute a value `time` from 0.0 to 1.0
                // inclusive that represents the ratio of `elapsed` time to the
                // `duration` of the VisualBell.
                let time = (elapsed_f / duration_f).min(1.0);

                // We use this to compute the inverse `intensity` of the
                // VisualBell. When `time` is 0.0, `inverse_intensity` is 0.0,
                // and when `time` is 1.0, `inverse_intensity` is 1.0.
                let inverse_intensity = match self.animation {
                    BellAnimation::Ease | BellAnimation::EaseOut => {
                        cubic_bezier(0.25, 0.1, 0.25, 1.0, time)
                    },
                    BellAnimation::EaseOutSine => cubic_bezier(0.39, 0.575, 0.565, 1.0, time),
                    BellAnimation::EaseOutQuad => cubic_bezier(0.25, 0.46, 0.45, 0.94, time),
                    BellAnimation::EaseOutCubic => cubic_bezier(0.215, 0.61, 0.355, 1.0, time),
                    BellAnimation::EaseOutQuart => cubic_bezier(0.165, 0.84, 0.44, 1.0, time),
                    BellAnimation::EaseOutQuint => cubic_bezier(0.23, 1.0, 0.32, 1.0, time),
                    BellAnimation::EaseOutExpo => cubic_bezier(0.19, 1.0, 0.22, 1.0, time),
                    BellAnimation::EaseOutCirc => cubic_bezier(0.075, 0.82, 0.165, 1.0, time),
                    BellAnimation::Linear => time,
                };

                // Since we want the `intensity` of the VisualBell to decay over
                // `time`, we subtract the `inverse_intensity` from 1.0.
                1.0 - inverse_intensity
            },
        }
    }

    pub fn update_config(&mut self, bell_config: &BellConfig) {
        self.animation = bell_config.animation;
        self.duration = bell_config.duration();
    }
}

impl From<&BellConfig> for VisualBell {
    fn from(bell_config: &BellConfig) -> VisualBell {
        VisualBell {
            animation: bell_config.animation,
            duration: bell_config.duration(),
            start_time: None,
        }
    }
}

fn cubic_bezier(p0: f64, p1: f64, p2: f64, p3: f64, x: f64) -> f64 {
    (1.0 - x).powi(3) * p0
        + 3.0 * (1.0 - x).powi(2) * x * p1
        + 3.0 * (1.0 - x) * x.powi(2) * p2
        + x.powi(3) * p3
}
