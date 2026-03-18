use super::WakeupGate;

#[derive(Default)]
struct WakeupHarness {
    gate: WakeupGate,
    occluded: bool,
    queued_wakeups: usize,
    redraws: usize,
}

impl WakeupHarness {
    fn send_wakeup(&mut self) {
        if self.gate.try_queue() {
            self.queued_wakeups += 1;
        }
    }

    fn set_occluded(&mut self, occluded: bool) {
        self.occluded = occluded;
    }

    fn redraw_if_visible(&mut self) {
        if self.occluded || !self.gate.is_pending() {
            return;
        }

        self.redraws += 1;
        if self.gate.on_redraw_complete() {
            self.queued_wakeups += 1;
        }
    }

    fn unrelated_redraw_if_visible(&mut self) {
        if self.occluded {
            return;
        }

        self.redraws += 1;
        if self.gate.on_redraw_complete() {
            self.queued_wakeups += 1;
        }
    }
}

#[derive(Default)]
struct ThrottledWakeupHarness {
    gate: WakeupGate,
    dirty: bool,
    occluded: bool,
    has_frame: bool,
    queued_wakeups: usize,
    redraws: usize,
    redraw_requested: bool,
}

impl ThrottledWakeupHarness {
    fn terminal_wakeup(&mut self) {
        if self.gate.try_queue() {
            self.queued_wakeups += 1;
        }
    }

    fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    fn process_one_wakeup_event(&mut self) {
        assert!(self.queued_wakeups > 0, "no queued wakeups to process");
        self.queued_wakeups -= 1;
        if !self.gate.is_pending() {
            return;
        }

        self.dirty = true;

        if self.has_frame {
            self.request_redraw();
        }
    }

    fn process_wakeup_events(&mut self) {
        while self.queued_wakeups > 0 {
            self.process_one_wakeup_event();
        }
    }

    fn frame_available(&mut self) {
        self.has_frame = true;
        if self.dirty {
            self.request_redraw();
        }
    }

    fn set_occluded(&mut self, occluded: bool) {
        self.occluded = occluded;
        if !occluded {
            self.dirty = true;
            self.request_redraw();
        }
    }

    fn unrelated_visible_draw(&mut self) {
        assert!(self.has_frame, "unrelated draw requires a frame token");
        assert!(!self.occluded, "unrelated visible draw requires a visible window");

        self.redraws += 1;
        self.has_frame = false;

        if self.gate.on_redraw_complete() {
            self.queued_wakeups += 1;
        }
    }

    fn draw_once(&mut self) {
        assert!(self.redraw_requested, "draw requested without a pending redraw");
        self.redraw_requested = false;

        if self.occluded {
            return;
        }

        self.redraws += 1;
        self.dirty = false;
        self.has_frame = false;

        if self.gate.on_redraw_complete() {
            self.queued_wakeups += 1;
        }
    }
}

#[test]
fn wakeup_gate_deduplicates_until_redraw() {
    let gate = WakeupGate::default();
    assert!(gate.try_queue());
    assert!(gate.is_pending());

    assert!(!gate.on_redraw_complete());
    assert!(!gate.is_pending());
    assert!(gate.try_queue());
}

#[test]
fn wakeup_gate_collapses_multiple_extra_wakeups_into_single_requeue() {
    let gate = WakeupGate::default();

    assert!(gate.try_queue());
    assert!(!gate.try_queue());
    assert!(!gate.try_queue());

    assert!(gate.on_redraw_complete());
    assert!(gate.is_pending());

    assert!(!gate.on_redraw_complete());
    assert!(!gate.is_pending());
}

#[test]
fn wakeup_gate_resets_after_failed_send_across_clones() {
    let gate = WakeupGate::default();
    let cloned = gate.clone();

    assert!(gate.try_queue());
    assert!(!cloned.try_queue());

    cloned.reset_after_send_failure();
    assert!(!gate.is_pending());
    assert!(gate.try_queue());
}

#[test]
fn occlusion_cycle_requeues_exactly_one_collapsed_wakeup() {
    let mut harness = WakeupHarness::default();

    // Initial wakeup queues the first frame.
    harness.send_wakeup();
    assert_eq!(harness.queued_wakeups, 1);
    assert!(harness.gate.is_pending());

    // While occluded, extra wakeups collapse into the pending frame.
    harness.set_occluded(true);
    harness.send_wakeup();
    harness.send_wakeup();
    assert_eq!(harness.queued_wakeups, 1, "occluded wakeups should be coalesced");
    assert!(harness.gate.is_pending());

    // No redraw may complete while the window stays occluded.
    harness.redraw_if_visible();
    assert_eq!(harness.redraws, 0);
    assert!(harness.gate.is_pending());

    // The first visible redraw must requeue exactly one collapsed wakeup.
    harness.set_occluded(false);
    harness.redraw_if_visible();
    assert_eq!(harness.redraws, 1);
    assert_eq!(harness.queued_wakeups, 2, "exactly one wakeup should be requeued");
    assert!(harness.gate.is_pending());

    // The requeued redraw clears the gate back to idle.
    harness.redraw_if_visible();
    assert_eq!(harness.redraws, 2);
    assert_eq!(harness.queued_wakeups, 2);
    assert!(!harness.gate.is_pending());
}

#[test]
fn occlusion_cycle_coalesces_new_wakeups_after_first_visible_redraw() {
    let mut harness = WakeupHarness::default();

    // Queue the initial wakeup, then collapse a couple more while occluded.
    harness.send_wakeup();
    harness.set_occluded(true);
    harness.send_wakeup();
    harness.send_wakeup();
    assert_eq!(harness.queued_wakeups, 1);
    assert!(harness.gate.is_pending());

    // The first visible redraw requeues exactly one collapsed wakeup.
    harness.set_occluded(false);
    harness.redraw_if_visible();
    assert_eq!(harness.redraws, 1);
    assert_eq!(harness.queued_wakeups, 2);
    assert!(harness.gate.is_pending());

    // If more wakeups arrive before that requeued redraw completes, they
    // must collapse into exactly one additional redraw.
    harness.send_wakeup();
    harness.send_wakeup();
    assert_eq!(harness.queued_wakeups, 2);
    assert!(harness.gate.is_pending());

    harness.redraw_if_visible();
    assert_eq!(harness.redraws, 2);
    assert_eq!(harness.queued_wakeups, 3);
    assert!(harness.gate.is_pending());

    harness.redraw_if_visible();
    assert_eq!(harness.redraws, 3);
    assert_eq!(harness.queued_wakeups, 3);
    assert!(!harness.gate.is_pending());
}

#[test]
fn unrelated_redraw_requeues_collapsed_wakeup() {
    let mut harness = WakeupHarness::default();

    harness.send_wakeup();
    harness.send_wakeup();
    assert_eq!(harness.queued_wakeups, 1);
    assert!(harness.gate.is_pending());

    // A redraw from another source should still complete the pending frame
    // and requeue exactly one coalesced wakeup.
    harness.unrelated_redraw_if_visible();
    assert_eq!(harness.redraws, 1);
    assert_eq!(harness.queued_wakeups, 2);
    assert!(harness.gate.is_pending());

    harness.redraw_if_visible();
    assert_eq!(harness.redraws, 2);
    assert_eq!(harness.queued_wakeups, 2);
    assert!(!harness.gate.is_pending());
}

#[test]
fn wakeup_gate_resets_cleanly_across_multiple_occlusion_cycles() {
    let mut harness = WakeupHarness::default();

    for cycle in 0..3 {
        harness.send_wakeup();
        harness.set_occluded(true);
        harness.send_wakeup();
        harness.send_wakeup();

        // Occlusion must prevent redraw completion from consuming the gate.
        harness.redraw_if_visible();
        assert_eq!(harness.redraws, cycle * 2);
        assert!(harness.gate.is_pending(), "gate should remain armed while occluded");

        harness.set_occluded(false);
        harness.redraw_if_visible();
        harness.redraw_if_visible();

        assert_eq!(harness.redraws, (cycle + 1) * 2);
        assert_eq!(harness.queued_wakeups, (cycle + 1) * 2);
        assert!(
            !harness.gate.is_pending(),
            "gate should return to idle after the requeued redraw completes"
        );
    }
}

#[test]
fn frame_throttling_and_occlusion_preserve_collapsed_wakeups() {
    let mut harness = ThrottledWakeupHarness { has_frame: true, ..Default::default() };

    harness.terminal_wakeup();
    harness.process_wakeup_events();
    assert!(harness.redraw_requested);
    harness.draw_once();
    assert_eq!(harness.redraws, 1);
    assert!(!harness.gate.is_pending());

    harness.set_occluded(true);
    harness.terminal_wakeup();
    harness.terminal_wakeup();
    assert_eq!(harness.queued_wakeups, 1, "collapsed wakeups should queue only once");

    harness.process_wakeup_events();
    assert!(harness.dirty);
    assert!(!harness.redraw_requested, "wakeups must wait for a frame while throttled");

    harness.frame_available();
    assert!(harness.redraw_requested);
    harness.draw_once();
    assert_eq!(harness.redraws, 1, "occluded redraws must not complete a frame");
    assert!(harness.gate.is_pending(), "gate must stay armed while occluded");

    harness.set_occluded(false);
    assert!(harness.redraw_requested, "unoccluding should request a redraw");
    harness.draw_once();
    assert_eq!(harness.redraws, 2);
    assert!(harness.gate.is_pending(), "first visible draw should requeue collapsed wakeup");
    assert_eq!(harness.queued_wakeups, 1);

    harness.process_wakeup_events();
    assert!(harness.dirty);
    assert!(!harness.redraw_requested, "requeued wakeup still waits for the next frame");

    harness.frame_available();
    assert!(harness.redraw_requested);
    harness.draw_once();
    assert_eq!(harness.redraws, 3);
    assert!(!harness.gate.is_pending());
}

#[test]
fn occluded_redraw_attempt_preserves_frame_and_requeues_on_first_visible_draw() {
    let mut harness = ThrottledWakeupHarness { has_frame: true, ..Default::default() };

    // Establish the normal visible-draw baseline.
    harness.terminal_wakeup();
    harness.process_wakeup_events();
    assert!(harness.redraw_requested);
    harness.draw_once();
    assert_eq!(harness.redraws, 1);
    assert!(!harness.gate.is_pending());

    // Collapse multiple wakeups while occluded and waiting for the next frame.
    harness.set_occluded(true);
    harness.terminal_wakeup();
    harness.terminal_wakeup();
    harness.process_wakeup_events();
    assert!(harness.dirty);
    assert!(!harness.redraw_requested);

    // When a frame becomes available, winit may still deliver a redraw
    // request while the window remains occluded. That attempt must not
    // consume the gate or lose the dirty/frame state.
    harness.frame_available();
    assert!(harness.redraw_requested);
    harness.draw_once();
    assert_eq!(harness.redraws, 1, "occluded redraw must not count as a visible draw");
    assert!(harness.has_frame, "occluded redraw should leave the frame available");
    assert!(harness.dirty, "occluded redraw should keep the window dirty");
    assert!(harness.gate.is_pending(), "occluded redraw must not consume the wakeup gate");
    assert!(!harness.redraw_requested);

    // Once visible again, the first real draw must requeue exactly one
    // collapsed wakeup and clear frame availability for the next refresh.
    harness.set_occluded(false);
    assert!(harness.redraw_requested);
    harness.draw_once();
    assert_eq!(harness.redraws, 2);
    assert!(!harness.has_frame);
    assert_eq!(harness.queued_wakeups, 1, "first visible draw should requeue once");

    harness.process_wakeup_events();
    assert!(harness.dirty);
    assert!(
        !harness.redraw_requested,
        "requeued wakeup should still wait for the next frame token"
    );

    harness.frame_available();
    assert!(harness.redraw_requested);
    harness.draw_once();
    assert_eq!(harness.redraws, 3);
    assert!(!harness.gate.is_pending());
}

#[test]
fn stale_wakeup_event_is_ignored_after_unrelated_visible_draw() {
    let mut harness = ThrottledWakeupHarness { has_frame: true, ..Default::default() };

    harness.terminal_wakeup();
    assert_eq!(harness.queued_wakeups, 1);
    assert!(harness.gate.is_pending());

    harness.unrelated_visible_draw();
    assert_eq!(harness.redraws, 1);
    assert!(!harness.gate.is_pending());

    harness.process_one_wakeup_event();
    assert!(!harness.dirty, "stale wakeup should not mark the window dirty again");
    assert!(!harness.redraw_requested, "stale wakeup should not request another redraw");
    assert_eq!(harness.redraws, 1);
}
