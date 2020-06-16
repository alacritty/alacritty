//! Scheduler for emitting events at a specific time in the future.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use glutin::event::Event as GlutinEvent;

use crate::event::Event as AlacrittyEvent;

/// Event ID for scrolling during selection.
pub const SELECTION_SCROLLING_EVENT: u64 = 0;

/// Number of IDs reserved for manual creation, these will never be automatically generated.
const RESERVED_IDS: u64 = 4096;

type Event = GlutinEvent<'static, AlacrittyEvent>;

/// Scheduler tracking all pending timers.
pub struct Scheduler {
    timers: VecDeque<Timer>,
    next_id: u64,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self { timers: VecDeque::new(), next_id: RESERVED_IDS }
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process all pending timers.
    ///
    /// If there are still timers pending after all ready events have been processed, the closest
    /// pending deadline will be returned.
    pub fn update(&mut self, event_queue: &mut Vec<Event>) -> Option<Instant> {
        let now = Instant::now();
        while !self.timers.is_empty() && self.timers[0].deadline <= now {
            if let Some(timer) = self.timers.pop_front() {
                // Automatically repeat the event.
                if let Some(interval) = timer.interval {
                    self.schedule(timer.event.clone(), interval, true, Some(timer.id));
                }

                event_queue.push(timer.event);
            }
        }

        self.timers.get(0).map(|timer| timer.deadline)
    }

    /// Schedule a new event.
    pub fn schedule(
        &mut self,
        event: Event,
        interval: Duration,
        repeat: bool,
        id: Option<u64>,
    ) -> u64 {
        let deadline = Instant::now() + interval;

        // Get insert position in the schedule.
        let mut index = self.timers.len();
        loop {
            if index == 0 {
                break;
            }
            index -= 1;

            if self.timers[index].deadline < deadline {
                break;
            }
        }

        // Retrieve the next free ID.
        let id = match id {
            Some(id) => id,
            None => {
                self.next_id += 1;
                self.next_id
            },
        };

        // Set the automatic event repeat rate.
        let interval = if repeat { Some(interval) } else { None };

        self.timers.insert(index, Timer { interval, deadline, event, id });

        id
    }

    /// Cancel a scheduled event.
    pub fn unschedule(&mut self, id: u64) -> Option<Event> {
        let index = self.timers.iter().position(|timer| timer.id == id)?;
        self.timers.remove(index).map(|timer| timer.event)
    }

    /// Access a staged event by ID.
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Timer> {
        self.timers.iter_mut().find(|timer| timer.id == id)
    }
}

/// Event scheduled to be emitted at a specific time.
pub struct Timer {
    pub deadline: Instant,
    pub event: Event,

    interval: Option<Duration>,
    id: u64,
}
