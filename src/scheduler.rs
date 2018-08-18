use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use futures::sync::oneshot;
use tokio;
use tokio::prelude::*;
use tokio::timer::Interval;

use event::Event;

pub struct Scheduler {
    event_tx: Sender<Event>,
}

impl Scheduler {
    pub fn new(event_tx: Sender<Event>) -> Self {
        Scheduler { event_tx }
    }

    pub fn register(&self, event: Event, interval_duration: Duration) -> oneshot::Sender<()> {
        // Create channel for killing the timer
        let (sender, receiver) = oneshot::channel();

        // Spawn task to send events to the event loop
        let event_tx = self.event_tx.clone();
        let task = Interval::new(Instant::now(), interval_duration)
            .for_each(move |_| {
                let _ = event_tx.clone().send(event.clone());
                Ok(())
            }).then(|_| Ok(()))
            .select(receiver)
            .then(|_| Ok(()));

        thread::spawn(move || {
            tokio::run(task);
        });

        sender
    }
}
