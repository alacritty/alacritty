//! [`egui`] bindings for [`winit`](https://github.com/rust-windowing/winit).
//!
//! The library translates winit events to egui, handled copy/paste,
//! updates the cursor, open links clicked in egui, etc.
//!
//! ## Feature flags
#![cfg_attr(feature = "document-features", doc = document_features::document_features!())]
//!

#![allow(clippy::manual_range_contains)]

#[cfg(feature = "accesskit")]
pub use accesskit_winit;
pub use egui;
#[cfg(feature = "accesskit")]
use egui::accesskit;
use egui::{Pos2, Rect, Theme, Vec2, ViewportBuilder, ViewportCommand, ViewportId, ViewportInfo};
pub use winit;

pub mod clipboard;
mod window_settings;

pub use window_settings::WindowSettings;

use ahash::HashSet;
use raw_window_handle::HasDisplayHandle;

use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::ElementState,
    event_loop::ActiveEventLoop,
    window::{CursorGrabMode, Window, WindowButtons, WindowLevel},
};

pub fn screen_size_in_pixels(window: &Window) -> egui::Vec2 {
    let size = if cfg!(target_os = "ios") {
        // `outer_size` Includes the area behind the "dynamic island".
        // It is up to the eframe user to make sure the dynamic island doesn't cover anything important.
        // That will be easier once https://github.com/rust-windowing/winit/pull/3890 lands
        window.outer_size()
    } else {
        window.inner_size()
    };
    egui::vec2(size.width as f32, size.height as f32)
}

/// Calculate the `pixels_per_point` for a given window, given the current egui zoom factor
pub fn pixels_per_point(egui_ctx: &egui::Context, window: &Window) -> f32 {
    let native_pixels_per_point = window.scale_factor() as f32;
    let egui_zoom_factor = egui_ctx.zoom_factor();
    egui_zoom_factor * native_pixels_per_point
}

// ----------------------------------------------------------------------------

#[must_use]
#[derive(Clone, Copy, Debug, Default)]
pub struct EventResponse {
    /// If true, egui consumed this event, i.e. wants exclusive use of this event
    /// (e.g. a mouse click on an egui window, or entering text into a text field).
    ///
    /// For instance, if you use egui for a game, you should only
    /// pass on the events to your game when [`Self::consumed`] is `false`.
    ///
    /// Note that egui uses `tab` to move focus between elements, so this will always be `true` for tabs.
    pub consumed: bool,

    /// Do we need an egui refresh because of this event?
    pub repaint: bool,
}

// ----------------------------------------------------------------------------

/// Handles the integration between egui and a winit Window.
///
/// Instantiate one of these per viewport/window.
pub struct State {
    /// Shared clone.
    egui_ctx: egui::Context,

    viewport_id: ViewportId,
    start_time: web_time::Instant,
    egui_input: egui::RawInput,
    pointer_pos_in_points: Option<egui::Pos2>,
    any_pointer_button_down: bool,
    current_cursor_icon: Option<egui::CursorIcon>,

    clipboard: clipboard::Clipboard,

    /// If `true`, mouse inputs will be treated as touches.
    /// Useful for debugging touch support in egui.
    ///
    /// Creates duplicate touches, if real touch inputs are coming.
    simulate_touch_screen: bool,

    /// Is Some(…) when a touch is being translated to a pointer.
    ///
    /// Only one touch will be interpreted as pointer at any time.
    pointer_touch_id: Option<u64>,

    /// track ime state
    has_sent_ime_enabled: bool,

    #[cfg(feature = "accesskit")]
    accesskit: Option<accesskit_winit::Adapter>,

    allow_ime: bool,
    ime_rect_px: Option<egui::Rect>,
}

impl State {
    /// Construct a new instance
    pub fn new(
        egui_ctx: egui::Context,
        viewport_id: ViewportId,
        display_target: &dyn HasDisplayHandle,
        native_pixels_per_point: Option<f32>,
        theme: Option<winit::window::Theme>,
        max_texture_side: Option<usize>,
    ) -> Self {
        profiling::function_scope!();

        let egui_input = egui::RawInput {
            focused: false, // winit will tell us when we have focus
            ..Default::default()
        };

        let mut slf = Self {
            egui_ctx,
            viewport_id,
            start_time: web_time::Instant::now(),
            egui_input,
            pointer_pos_in_points: None,
            any_pointer_button_down: false,
            current_cursor_icon: None,

            clipboard: clipboard::Clipboard::new(
                display_target.display_handle().ok().map(|h| h.as_raw()),
            ),

            simulate_touch_screen: false,
            pointer_touch_id: None,

            has_sent_ime_enabled: false,

            #[cfg(feature = "accesskit")]
            accesskit: None,

            allow_ime: false,
            ime_rect_px: None,
        };

        slf.egui_input
            .viewports
            .entry(ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = native_pixels_per_point;
        slf.egui_input.system_theme = theme.map(to_egui_theme);

        if let Some(max_texture_side) = max_texture_side {
            slf.set_max_texture_side(max_texture_side);
        }
        slf
    }

    #[cfg(feature = "accesskit")]
    pub fn init_accesskit<T: From<accesskit_winit::Event> + Send>(
        &mut self,
        window: &Window,
        event_loop_proxy: winit::event_loop::EventLoopProxy<T>,
    ) {
        profiling::function_scope!();

        self.accesskit = Some(accesskit_winit::Adapter::with_event_loop_proxy(
            window,
            event_loop_proxy,
        ));
    }

    /// Call this once a graphics context has been created to update the maximum texture dimensions
    /// that egui will use.
    pub fn set_max_texture_side(&mut self, max_texture_side: usize) {
        self.egui_input.max_texture_side = Some(max_texture_side);
    }

    /// Fetches text from the clipboard and returns it.
    pub fn clipboard_text(&mut self) -> Option<String> {
        self.clipboard.get()
    }

    /// Places the text onto the clipboard.
    pub fn set_clipboard_text(&mut self, text: String) {
        self.clipboard.set_text(text);
    }

    /// Returns [`false`] or the last value that [`Window::set_ime_allowed()`] was called with, used for debouncing.
    pub fn allow_ime(&self) -> bool {
        self.allow_ime
    }

    /// Set the last value that [`Window::set_ime_allowed()`] was called with.
    pub fn set_allow_ime(&mut self, allow: bool) {
        self.allow_ime = allow;
    }

    #[inline]
    pub fn egui_ctx(&self) -> &egui::Context {
        &self.egui_ctx
    }

    /// The current input state.
    /// This is changed by [`Self::on_window_event`] and cleared by [`Self::take_egui_input`].
    #[inline]
    pub fn egui_input(&self) -> &egui::RawInput {
        &self.egui_input
    }

    /// The current input state.
    /// This is changed by [`Self::on_window_event`] and cleared by [`Self::take_egui_input`].
    #[inline]
    pub fn egui_input_mut(&mut self) -> &mut egui::RawInput {
        &mut self.egui_input
    }

    /// Prepare for a new frame by extracting the accumulated input,
    ///
    /// as well as setting [the time](egui::RawInput::time) and [screen rectangle](egui::RawInput::screen_rect).
    ///
    /// You need to set [`egui::RawInput::viewports`] yourself though.
    /// Use [`update_viewport_info`] to update the info for each
    /// viewport.
    pub fn take_egui_input(&mut self, window: &Window) -> egui::RawInput {
        profiling::function_scope!();

        self.egui_input.time = Some(self.start_time.elapsed().as_secs_f64());

        // On Windows, a minimized window will have 0 width and height.
        // See: https://github.com/rust-windowing/winit/issues/208
        // This solves an issue where egui window positions would be changed when minimizing on Windows.
        let screen_size_in_pixels = screen_size_in_pixels(window);
        let screen_size_in_points =
            screen_size_in_pixels / pixels_per_point(&self.egui_ctx, window);

        self.egui_input.screen_rect = (screen_size_in_points.x > 0.0
            && screen_size_in_points.y > 0.0)
            .then(|| Rect::from_min_size(Pos2::ZERO, screen_size_in_points));

        // Tell egui which viewport is now active:
        self.egui_input.viewport_id = self.viewport_id;

        self.egui_input
            .viewports
            .entry(self.viewport_id)
            .or_default()
            .native_pixels_per_point = Some(window.scale_factor() as f32);

        self.egui_input.take()
    }

    /// Call this when there is a new event.
    ///
    /// The result can be found in [`Self::egui_input`] and be extracted with [`Self::take_egui_input`].
    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &winit::event::WindowEvent,
    ) -> EventResponse {
        profiling::function_scope!(short_window_event_description(event));

        #[cfg(feature = "accesskit")]
        if let Some(accesskit) = self.accesskit.as_mut() {
            accesskit.process_event(window, event);
        }

        use winit::event::WindowEvent;
        match event {
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let native_pixels_per_point = *scale_factor as f32;

                self.egui_input
                    .viewports
                    .entry(self.viewport_id)
                    .or_default()
                    .native_pixels_per_point = Some(native_pixels_per_point);

                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.on_mouse_button_input(*state, *button);
                EventResponse {
                    repaint: true,
                    consumed: self.egui_ctx.wants_pointer_input(),
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.on_mouse_wheel(window, *delta);
                EventResponse {
                    repaint: true,
                    consumed: self.egui_ctx.wants_pointer_input(),
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.on_cursor_moved(window, *position);
                EventResponse {
                    repaint: true,
                    consumed: self.egui_ctx.is_using_pointer(),
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer_pos_in_points = None;
                self.egui_input.events.push(egui::Event::PointerGone);
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            // WindowEvent::TouchpadPressure {device_id, pressure, stage, ..  } => {} // TODO(emilk)
            WindowEvent::Touch(touch) => {
                self.on_touch(window, touch);
                let consumed = match touch.phase {
                    winit::event::TouchPhase::Started
                    | winit::event::TouchPhase::Ended
                    | winit::event::TouchPhase::Cancelled => self.egui_ctx.wants_pointer_input(),
                    winit::event::TouchPhase::Moved => self.egui_ctx.is_using_pointer(),
                };
                EventResponse {
                    repaint: true,
                    consumed,
                }
            }

            WindowEvent::Ime(ime) => {
                // on Mac even Cmd-C is pressed during ime, a `c` is pushed to Preedit.
                // So no need to check is_mac_cmd.
                //
                // How winit produce `Ime::Enabled` and `Ime::Disabled` differs in MacOS
                // and Windows.
                //
                // - On Windows, before and after each Commit will produce an Enable/Disabled
                // event.
                // - On MacOS, only when user explicit enable/disable ime. No Disabled
                // after Commit.
                //
                // We use input_method_editor_started to manually insert CompositionStart
                // between Commits.
                match ime {
                    winit::event::Ime::Enabled => {
                        if cfg!(target_os = "linux") {
                            // This event means different things in X11 and Wayland, but we can just
                            // ignore it and enable IME on the preedit event.
                            // See <https://github.com/rust-windowing/winit/issues/2498>
                        } else {
                            self.ime_event_enable();
                        }
                    }
                    winit::event::Ime::Preedit(text, Some(_cursor)) => {
                        self.ime_event_enable();
                        self.egui_input
                            .events
                            .push(egui::Event::Ime(egui::ImeEvent::Preedit(text.clone())));
                    }
                    winit::event::Ime::Commit(text) => {
                        self.egui_input
                            .events
                            .push(egui::Event::Ime(egui::ImeEvent::Commit(text.clone())));
                        self.ime_event_disable();
                    }
                    winit::event::Ime::Disabled | winit::event::Ime::Preedit(_, None) => {
                        self.ime_event_disable();
                    }
                };

                EventResponse {
                    repaint: true,
                    consumed: self.egui_ctx.wants_keyboard_input(),
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                // Winit generates fake "synthetic" KeyboardInput events when the focus
                // is changed to the window, or away from it. Synthetic key presses
                // represent no real key presses and should be ignored.
                // See https://github.com/rust-windowing/winit/issues/3543
                if *is_synthetic && event.state == ElementState::Pressed {
                    EventResponse {
                        repaint: true,
                        consumed: false,
                    }
                } else {
                    self.on_keyboard_input(event);

                    // When pressing the Tab key, egui focuses the first focusable element, hence Tab always consumes.
                    let consumed = self.egui_ctx.wants_keyboard_input()
                        || event.logical_key
                            == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Tab);
                    EventResponse {
                        repaint: true,
                        consumed,
                    }
                }
            }
            WindowEvent::Focused(focused) => {
                self.egui_input.focused = *focused;
                self.egui_input
                    .events
                    .push(egui::Event::WindowFocused(*focused));
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::ThemeChanged(winit_theme) => {
                self.egui_input.system_theme = Some(to_egui_theme(*winit_theme));
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::HoveredFile(path) => {
                self.egui_input.hovered_files.push(egui::HoveredFile {
                    path: Some(path.clone()),
                    ..Default::default()
                });
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::HoveredFileCancelled => {
                self.egui_input.hovered_files.clear();
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::DroppedFile(path) => {
                self.egui_input.hovered_files.clear();
                self.egui_input.dropped_files.push(egui::DroppedFile {
                    path: Some(path.clone()),
                    ..Default::default()
                });
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::ModifiersChanged(state) => {
                let state = state.state();

                let alt = state.alt_key();
                let ctrl = state.control_key();
                let shift = state.shift_key();
                let super_ = state.super_key();

                self.egui_input.modifiers.alt = alt;
                self.egui_input.modifiers.ctrl = ctrl;
                self.egui_input.modifiers.shift = shift;
                self.egui_input.modifiers.mac_cmd = cfg!(target_os = "macos") && super_;
                self.egui_input.modifiers.command = if cfg!(target_os = "macos") {
                    super_
                } else {
                    ctrl
                };

                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }

            // Things that may require repaint:
            WindowEvent::RedrawRequested
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::Destroyed
            | WindowEvent::Occluded(_)
            | WindowEvent::Resized(_)
            | WindowEvent::Moved(_)
            | WindowEvent::TouchpadPressure { .. }
            | WindowEvent::CloseRequested => EventResponse {
                repaint: true,
                consumed: false,
            },

            // Things we completely ignore:
            WindowEvent::ActivationTokenDone { .. }
            | WindowEvent::AxisMotion { .. }
            | WindowEvent::DoubleTapGesture { .. }
            | WindowEvent::RotationGesture { .. }
            | WindowEvent::PanGesture { .. } => EventResponse {
                repaint: false,
                consumed: false,
            },

            WindowEvent::PinchGesture { delta, .. } => {
                // Positive delta values indicate magnification (zooming in).
                // Negative delta values indicate shrinking (zooming out).
                let zoom_factor = (*delta as f32).exp();
                self.egui_input.events.push(egui::Event::Zoom(zoom_factor));
                EventResponse {
                    repaint: true,
                    consumed: self.egui_ctx.wants_pointer_input(),
                }
            }
        }
    }

    pub fn ime_event_enable(&mut self) {
        if !self.has_sent_ime_enabled {
            self.egui_input
                .events
                .push(egui::Event::Ime(egui::ImeEvent::Enabled));
            self.has_sent_ime_enabled = true;
        }
    }

    pub fn ime_event_disable(&mut self) {
        self.egui_input
            .events
            .push(egui::Event::Ime(egui::ImeEvent::Disabled));
        self.has_sent_ime_enabled = false;
    }

    pub fn on_mouse_motion(&mut self, delta: (f64, f64)) {
        self.egui_input.events.push(egui::Event::MouseMoved(Vec2 {
            x: delta.0 as f32,
            y: delta.1 as f32,
        }));
    }

    /// Call this when there is a new [`accesskit::ActionRequest`].
    ///
    /// The result can be found in [`Self::egui_input`] and be extracted with [`Self::take_egui_input`].
    #[cfg(feature = "accesskit")]
    pub fn on_accesskit_action_request(&mut self, request: accesskit::ActionRequest) {
        self.egui_input
            .events
            .push(egui::Event::AccessKitActionRequest(request));
    }

    fn on_mouse_button_input(
        &mut self,
        state: winit::event::ElementState,
        button: winit::event::MouseButton,
    ) {
        if let Some(pos) = self.pointer_pos_in_points {
            if let Some(button) = translate_mouse_button(button) {
                let pressed = state == winit::event::ElementState::Pressed;

                self.egui_input.events.push(egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: self.egui_input.modifiers,
                });

                if self.simulate_touch_screen {
                    if pressed {
                        self.any_pointer_button_down = true;

                        self.egui_input.events.push(egui::Event::Touch {
                            device_id: egui::TouchDeviceId(0),
                            id: egui::TouchId(0),
                            phase: egui::TouchPhase::Start,
                            pos,
                            force: None,
                        });
                    } else {
                        self.any_pointer_button_down = false;

                        self.egui_input.events.push(egui::Event::PointerGone);

                        self.egui_input.events.push(egui::Event::Touch {
                            device_id: egui::TouchDeviceId(0),
                            id: egui::TouchId(0),
                            phase: egui::TouchPhase::End,
                            pos,
                            force: None,
                        });
                    };
                }
            }
        }
    }

    fn on_cursor_moved(
        &mut self,
        window: &Window,
        pos_in_pixels: winit::dpi::PhysicalPosition<f64>,
    ) {
        let pixels_per_point = pixels_per_point(&self.egui_ctx, window);

        let pos_in_points = egui::pos2(
            pos_in_pixels.x as f32 / pixels_per_point,
            pos_in_pixels.y as f32 / pixels_per_point,
        );
        self.pointer_pos_in_points = Some(pos_in_points);

        if self.simulate_touch_screen {
            if self.any_pointer_button_down {
                self.egui_input
                    .events
                    .push(egui::Event::PointerMoved(pos_in_points));

                self.egui_input.events.push(egui::Event::Touch {
                    device_id: egui::TouchDeviceId(0),
                    id: egui::TouchId(0),
                    phase: egui::TouchPhase::Move,
                    pos: pos_in_points,
                    force: None,
                });
            }
        } else {
            self.egui_input
                .events
                .push(egui::Event::PointerMoved(pos_in_points));
        }
    }

    fn on_touch(&mut self, window: &Window, touch: &winit::event::Touch) {
        let pixels_per_point = pixels_per_point(&self.egui_ctx, window);

        // Emit touch event
        self.egui_input.events.push(egui::Event::Touch {
            device_id: egui::TouchDeviceId(egui::epaint::util::hash(touch.device_id)),
            id: egui::TouchId::from(touch.id),
            phase: match touch.phase {
                winit::event::TouchPhase::Started => egui::TouchPhase::Start,
                winit::event::TouchPhase::Moved => egui::TouchPhase::Move,
                winit::event::TouchPhase::Ended => egui::TouchPhase::End,
                winit::event::TouchPhase::Cancelled => egui::TouchPhase::Cancel,
            },
            pos: egui::pos2(
                touch.location.x as f32 / pixels_per_point,
                touch.location.y as f32 / pixels_per_point,
            ),
            force: match touch.force {
                Some(winit::event::Force::Normalized(force)) => Some(force as f32),
                Some(winit::event::Force::Calibrated {
                    force,
                    max_possible_force,
                    ..
                }) => Some((force / max_possible_force) as f32),
                None => None,
            },
        });
        // If we're not yet translating a touch or we're translating this very
        // touch …
        if self.pointer_touch_id.is_none() || self.pointer_touch_id.unwrap_or_default() == touch.id
        {
            // … emit PointerButton resp. PointerMoved events to emulate mouse
            match touch.phase {
                winit::event::TouchPhase::Started => {
                    self.pointer_touch_id = Some(touch.id);
                    // First move the pointer to the right location
                    self.on_cursor_moved(window, touch.location);
                    self.on_mouse_button_input(
                        winit::event::ElementState::Pressed,
                        winit::event::MouseButton::Left,
                    );
                }
                winit::event::TouchPhase::Moved => {
                    self.on_cursor_moved(window, touch.location);
                }
                winit::event::TouchPhase::Ended => {
                    self.pointer_touch_id = None;
                    self.on_mouse_button_input(
                        winit::event::ElementState::Released,
                        winit::event::MouseButton::Left,
                    );
                    // The pointer should vanish completely to not get any
                    // hover effects
                    self.pointer_pos_in_points = None;
                    self.egui_input.events.push(egui::Event::PointerGone);
                }
                winit::event::TouchPhase::Cancelled => {
                    self.pointer_touch_id = None;
                    self.pointer_pos_in_points = None;
                    self.egui_input.events.push(egui::Event::PointerGone);
                }
            }
        }
    }

    fn on_mouse_wheel(&mut self, window: &Window, delta: winit::event::MouseScrollDelta) {
        let pixels_per_point = pixels_per_point(&self.egui_ctx, window);

        {
            let (unit, delta) = match delta {
                winit::event::MouseScrollDelta::LineDelta(x, y) => {
                    (egui::MouseWheelUnit::Line, egui::vec2(x, y))
                }
                winit::event::MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition {
                    x,
                    y,
                }) => (
                    egui::MouseWheelUnit::Point,
                    egui::vec2(x as f32, y as f32) / pixels_per_point,
                ),
            };
            let modifiers = self.egui_input.modifiers;
            self.egui_input.events.push(egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
            });
        }
    }

    fn on_keyboard_input(&mut self, event: &winit::event::KeyEvent) {
        let winit::event::KeyEvent {
            // Represents the position of a key independent of the currently active layout.
            //
            // It also uniquely identifies the physical key (i.e. it's mostly synonymous with a scancode).
            // The most prevalent use case for this is games. For example the default keys for the player
            // to move around might be the W, A, S, and D keys on a US layout. The position of these keys
            // is more important than their label, so they should map to Z, Q, S, and D on an "AZERTY"
            // layout. (This value is `KeyCode::KeyW` for the Z key on an AZERTY layout.)
            physical_key,

            // Represents the results of a keymap, i.e. what character a certain key press represents.
            // When telling users "Press Ctrl-F to find", this is where we should
            // look for the "F" key, because they may have a dvorak layout on
            // a qwerty keyboard, and so the logical "F" character may not be located on the physical `KeyCode::KeyF` position.
            logical_key,

            text,

            state,

            location: _, // e.g. is it on the numpad?
            repeat: _,   // egui will figure this out for us
            ..
        } = event;

        let pressed = *state == winit::event::ElementState::Pressed;

        let physical_key = if let winit::keyboard::PhysicalKey::Code(keycode) = *physical_key {
            key_from_key_code(keycode)
        } else {
            None
        };

        let logical_key = key_from_winit_key(logical_key);

        // Helpful logging to enable when adding new key support
        log::trace!(
            "logical {:?} -> {:?},  physical {:?} -> {:?}",
            event.logical_key,
            logical_key,
            event.physical_key,
            physical_key
        );

        // "Logical OR physical key" is a fallback mechanism for keyboard layouts without Latin characters: it lets them
        // emit events as if the corresponding keys from the Latin layout were pressed. In this case, clipboard shortcuts
        // are mapped to the physical keys that normally contain C, X, V, etc.
        // See also: https://github.com/emilk/egui/issues/3653
        if let Some(active_key) = logical_key.or(physical_key) {
            if pressed {
                if is_cut_command(self.egui_input.modifiers, active_key) {
                    self.egui_input.events.push(egui::Event::Cut);
                    return;
                } else if is_copy_command(self.egui_input.modifiers, active_key) {
                    self.egui_input.events.push(egui::Event::Copy);
                    return;
                } else if is_paste_command(self.egui_input.modifiers, active_key) {
                    // alacritree: fall through to Key event when the clipboard
                    // had no text, so terminal apps still see raw 0x16.
                    if let Some(contents) = self.clipboard.get() {
                        let contents = contents.replace("\r\n", "\n");
                        if !contents.is_empty() {
                            self.egui_input.events.push(egui::Event::Paste(contents));
                            return;
                        }
                    }
                }
            }

            self.egui_input.events.push(egui::Event::Key {
                key: active_key,
                physical_key,
                pressed,
                repeat: false, // egui will fill this in for us!
                modifiers: self.egui_input.modifiers,
            });
        }

        if let Some(text) = &text {
            // Make sure there is text, and that it is not control characters
            // (e.g. delete is sent as "\u{f728}" on macOS).
            if !text.is_empty() && text.chars().all(is_printable_char) {
                // On some platforms we get here when the user presses Cmd-C (copy), ctrl-W, etc.
                // We need to ignore these characters that are side-effects of commands.
                // Also make sure the key is pressed (not released). On Linux, text might
                // contain some data even when the key is released.
                let is_cmd = self.egui_input.modifiers.ctrl
                    || self.egui_input.modifiers.command
                    || self.egui_input.modifiers.mac_cmd;
                if pressed && !is_cmd {
                    self.egui_input
                        .events
                        .push(egui::Event::Text(text.to_string()));
                }
            }
        }
    }

    /// Call with the output given by `egui`.
    ///
    /// This will, if needed:
    /// * update the cursor
    /// * copy text to the clipboard
    /// * open any clicked urls
    /// * update the IME
    /// *
    pub fn handle_platform_output(
        &mut self,
        window: &Window,
        platform_output: egui::PlatformOutput,
    ) {
        #![allow(deprecated)]
        profiling::function_scope!();

        let egui::PlatformOutput {
            commands,
            cursor_icon,
            open_url,
            copied_text,
            events: _,                    // handled elsewhere
            mutable_text_under_cursor: _, // only used in eframe web
            ime,
            #[cfg(feature = "accesskit")]
            accesskit_update,
            num_completed_passes: _,    // `egui::Context::run` handles this
            request_discard_reasons: _, // `egui::Context::run` handles this
        } = platform_output;

        for command in commands {
            match command {
                egui::OutputCommand::CopyText(text) => {
                    self.clipboard.set_text(text);
                }
                egui::OutputCommand::CopyImage(image) => {
                    self.clipboard.set_image(&image);
                }
                egui::OutputCommand::OpenUrl(open_url) => {
                    open_url_in_browser(&open_url.url);
                }
            }
        }

        self.set_cursor_icon(window, cursor_icon);

        if let Some(open_url) = open_url {
            open_url_in_browser(&open_url.url);
        }

        if !copied_text.is_empty() {
            self.clipboard.set_text(copied_text);
        }

        let allow_ime = ime.is_some();
        if self.allow_ime != allow_ime {
            self.allow_ime = allow_ime;
            profiling::scope!("set_ime_allowed");
            window.set_ime_allowed(allow_ime);
        }

        if let Some(ime) = ime {
            let pixels_per_point = pixels_per_point(&self.egui_ctx, window);
            let ime_rect_px = pixels_per_point * ime.rect;
            if self.ime_rect_px != Some(ime_rect_px)
                || self.egui_ctx.input(|i| !i.events.is_empty())
            {
                self.ime_rect_px = Some(ime_rect_px);
                profiling::scope!("set_ime_cursor_area");
                window.set_ime_cursor_area(
                    winit::dpi::PhysicalPosition {
                        x: ime_rect_px.min.x,
                        y: ime_rect_px.min.y,
                    },
                    winit::dpi::PhysicalSize {
                        width: ime_rect_px.width(),
                        height: ime_rect_px.height(),
                    },
                );
            }
        } else {
            self.ime_rect_px = None;
        }

        #[cfg(feature = "accesskit")]
        if let Some(accesskit) = self.accesskit.as_mut() {
            if let Some(update) = accesskit_update {
                profiling::scope!("accesskit");
                accesskit.update_if_active(|| update);
            }
        }
    }

    fn set_cursor_icon(&mut self, window: &Window, cursor_icon: egui::CursorIcon) {
        if self.current_cursor_icon == Some(cursor_icon) {
            // Prevent flickering near frame boundary when Windows OS tries to control cursor icon for window resizing.
            // On other platforms: just early-out to save CPU.
            return;
        }

        let is_pointer_in_window = self.pointer_pos_in_points.is_some();
        if is_pointer_in_window {
            self.current_cursor_icon = Some(cursor_icon);

            if let Some(winit_cursor_icon) = translate_cursor(cursor_icon) {
                window.set_cursor_visible(true);
                window.set_cursor(winit_cursor_icon);
            } else {
                window.set_cursor_visible(false);
            }
        } else {
            // Remember to set the cursor again once the cursor returns to the screen:
            self.current_cursor_icon = None;
        }
    }
}

fn to_egui_theme(theme: winit::window::Theme) -> Theme {
    match theme {
        winit::window::Theme::Dark => Theme::Dark,
        winit::window::Theme::Light => Theme::Light,
    }
}

pub fn inner_rect_in_points(window: &Window, pixels_per_point: f32) -> Option<Rect> {
    let inner_pos_px = window.inner_position().ok()?;
    let inner_pos_px = egui::pos2(inner_pos_px.x as f32, inner_pos_px.y as f32);

    let inner_size_px = window.inner_size();
    let inner_size_px = egui::vec2(inner_size_px.width as f32, inner_size_px.height as f32);

    let inner_rect_px = egui::Rect::from_min_size(inner_pos_px, inner_size_px);

    Some(inner_rect_px / pixels_per_point)
}

pub fn outer_rect_in_points(window: &Window, pixels_per_point: f32) -> Option<Rect> {
    let outer_pos_px = window.outer_position().ok()?;
    let outer_pos_px = egui::pos2(outer_pos_px.x as f32, outer_pos_px.y as f32);

    let outer_size_px = window.outer_size();
    let outer_size_px = egui::vec2(outer_size_px.width as f32, outer_size_px.height as f32);

    let outer_rect_px = egui::Rect::from_min_size(outer_pos_px, outer_size_px);

    Some(outer_rect_px / pixels_per_point)
}

/// Update the given viewport info with the current state of the window.
///
/// Call before [`State::take_egui_input`].
///
/// If this is called right after window creation, `is_init` should be `true`, otherwise `false`.
pub fn update_viewport_info(
    viewport_info: &mut ViewportInfo,
    egui_ctx: &egui::Context,
    window: &Window,
    is_init: bool,
) {
    profiling::function_scope!();
    let pixels_per_point = pixels_per_point(egui_ctx, window);

    let has_a_position = match window.is_minimized() {
        Some(true) => false,
        Some(false) | None => true,
    };

    let inner_rect = if has_a_position {
        inner_rect_in_points(window, pixels_per_point)
    } else {
        None
    };

    let outer_rect = if has_a_position {
        outer_rect_in_points(window, pixels_per_point)
    } else {
        None
    };

    let monitor_size = {
        profiling::scope!("monitor_size");
        if let Some(monitor) = window.current_monitor() {
            let size = monitor.size().to_logical::<f32>(pixels_per_point.into());
            Some(egui::vec2(size.width, size.height))
        } else {
            None
        }
    };

    viewport_info.title = Some(window.title());
    viewport_info.native_pixels_per_point = Some(window.scale_factor() as f32);

    viewport_info.monitor_size = monitor_size;
    viewport_info.inner_rect = inner_rect;
    viewport_info.outer_rect = outer_rect;

    if is_init || !cfg!(target_os = "macos") {
        // Asking for minimized/maximized state at runtime leads to a deadlock on Mac when running
        // `cargo run -p custom_window_frame`.
        // See https://github.com/emilk/egui/issues/3494
        viewport_info.maximized = Some(window.is_maximized());
        viewport_info.minimized = Some(window.is_minimized().unwrap_or(false));
    }

    viewport_info.fullscreen = Some(window.fullscreen().is_some());
    viewport_info.focused = Some(window.has_focus());
}

fn open_url_in_browser(_url: &str) {
    #[cfg(feature = "webbrowser")]
    if let Err(err) = webbrowser::open(_url) {
        log::warn!("Failed to open url: {}", err);
    }

    #[cfg(not(feature = "webbrowser"))]
    {
        log::warn!("Cannot open url - feature \"links\" not enabled.");
    }
}

/// Winit sends special keys (backspace, delete, F1, …) as characters.
/// Ignore those.
/// We also ignore '\r', '\n', '\t'.
/// Newlines are handled by the `Key::Enter` event.
fn is_printable_char(chr: char) -> bool {
    let is_in_private_use_area = '\u{e000}' <= chr && chr <= '\u{f8ff}'
        || '\u{f0000}' <= chr && chr <= '\u{ffffd}'
        || '\u{100000}' <= chr && chr <= '\u{10fffd}';

    !is_in_private_use_area && !chr.is_ascii_control()
}

fn is_cut_command(modifiers: egui::Modifiers, keycode: egui::Key) -> bool {
    keycode == egui::Key::Cut
        || (modifiers.command && keycode == egui::Key::X)
        || (cfg!(target_os = "windows") && modifiers.shift && keycode == egui::Key::Delete)
}

fn is_copy_command(modifiers: egui::Modifiers, keycode: egui::Key) -> bool {
    keycode == egui::Key::Copy
        || (modifiers.command && keycode == egui::Key::C)
        || (cfg!(target_os = "windows") && modifiers.ctrl && keycode == egui::Key::Insert)
}

fn is_paste_command(modifiers: egui::Modifiers, keycode: egui::Key) -> bool {
    keycode == egui::Key::Paste
        || (modifiers.command && keycode == egui::Key::V)
        || (cfg!(target_os = "windows") && modifiers.shift && keycode == egui::Key::Insert)
}

fn translate_mouse_button(button: winit::event::MouseButton) -> Option<egui::PointerButton> {
    match button {
        winit::event::MouseButton::Left => Some(egui::PointerButton::Primary),
        winit::event::MouseButton::Right => Some(egui::PointerButton::Secondary),
        winit::event::MouseButton::Middle => Some(egui::PointerButton::Middle),
        winit::event::MouseButton::Back => Some(egui::PointerButton::Extra1),
        winit::event::MouseButton::Forward => Some(egui::PointerButton::Extra2),
        winit::event::MouseButton::Other(_) => None,
    }
}

fn key_from_winit_key(key: &winit::keyboard::Key) -> Option<egui::Key> {
    match key {
        winit::keyboard::Key::Named(named_key) => key_from_named_key(*named_key),
        winit::keyboard::Key::Character(str) => egui::Key::from_name(str.as_str()),
        winit::keyboard::Key::Unidentified(_) | winit::keyboard::Key::Dead(_) => None,
    }
}

fn key_from_named_key(named_key: winit::keyboard::NamedKey) -> Option<egui::Key> {
    use egui::Key;
    use winit::keyboard::NamedKey;

    Some(match named_key {
        NamedKey::Enter => Key::Enter,
        NamedKey::Tab => Key::Tab,
        NamedKey::ArrowDown => Key::ArrowDown,
        NamedKey::ArrowLeft => Key::ArrowLeft,
        NamedKey::ArrowRight => Key::ArrowRight,
        NamedKey::ArrowUp => Key::ArrowUp,
        NamedKey::End => Key::End,
        NamedKey::Home => Key::Home,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        NamedKey::Insert => Key::Insert,
        NamedKey::Escape => Key::Escape,
        NamedKey::Cut => Key::Cut,
        NamedKey::Copy => Key::Copy,
        NamedKey::Paste => Key::Paste,

        NamedKey::Space => Key::Space,

        NamedKey::F1 => Key::F1,
        NamedKey::F2 => Key::F2,
        NamedKey::F3 => Key::F3,
        NamedKey::F4 => Key::F4,
        NamedKey::F5 => Key::F5,
        NamedKey::F6 => Key::F6,
        NamedKey::F7 => Key::F7,
        NamedKey::F8 => Key::F8,
        NamedKey::F9 => Key::F9,
        NamedKey::F10 => Key::F10,
        NamedKey::F11 => Key::F11,
        NamedKey::F12 => Key::F12,
        NamedKey::F13 => Key::F13,
        NamedKey::F14 => Key::F14,
        NamedKey::F15 => Key::F15,
        NamedKey::F16 => Key::F16,
        NamedKey::F17 => Key::F17,
        NamedKey::F18 => Key::F18,
        NamedKey::F19 => Key::F19,
        NamedKey::F20 => Key::F20,
        NamedKey::F21 => Key::F21,
        NamedKey::F22 => Key::F22,
        NamedKey::F23 => Key::F23,
        NamedKey::F24 => Key::F24,
        NamedKey::F25 => Key::F25,
        NamedKey::F26 => Key::F26,
        NamedKey::F27 => Key::F27,
        NamedKey::F28 => Key::F28,
        NamedKey::F29 => Key::F29,
        NamedKey::F30 => Key::F30,
        NamedKey::F31 => Key::F31,
        NamedKey::F32 => Key::F32,
        NamedKey::F33 => Key::F33,
        NamedKey::F34 => Key::F34,
        NamedKey::F35 => Key::F35,
        _ => {
            log::trace!("Unknown key: {named_key:?}");
            return None;
        }
    })
}

fn key_from_key_code(key: winit::keyboard::KeyCode) -> Option<egui::Key> {
    use egui::Key;
    use winit::keyboard::KeyCode;

    Some(match key {
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::ArrowUp => Key::ArrowUp,

        KeyCode::Escape => Key::Escape,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,

        KeyCode::Insert => Key::Insert,
        KeyCode::Delete => Key::Delete,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,

        // Punctuation
        KeyCode::Space => Key::Space,
        KeyCode::Comma => Key::Comma,
        KeyCode::Period => Key::Period,
        // KeyCode::Colon => Key::Colon, // NOTE: there is no physical colon key on an american keyboard
        KeyCode::Semicolon => Key::Semicolon,
        KeyCode::Backslash => Key::Backslash,
        KeyCode::Slash | KeyCode::NumpadDivide => Key::Slash,
        KeyCode::BracketLeft => Key::OpenBracket,
        KeyCode::BracketRight => Key::CloseBracket,
        KeyCode::Backquote => Key::Backtick,
        KeyCode::Quote => Key::Quote,

        KeyCode::Cut => Key::Cut,
        KeyCode::Copy => Key::Copy,
        KeyCode::Paste => Key::Paste,
        KeyCode::Minus | KeyCode::NumpadSubtract => Key::Minus,
        KeyCode::NumpadAdd => Key::Plus,
        KeyCode::Equal => Key::Equals,

        KeyCode::Digit0 | KeyCode::Numpad0 => Key::Num0,
        KeyCode::Digit1 | KeyCode::Numpad1 => Key::Num1,
        KeyCode::Digit2 | KeyCode::Numpad2 => Key::Num2,
        KeyCode::Digit3 | KeyCode::Numpad3 => Key::Num3,
        KeyCode::Digit4 | KeyCode::Numpad4 => Key::Num4,
        KeyCode::Digit5 | KeyCode::Numpad5 => Key::Num5,
        KeyCode::Digit6 | KeyCode::Numpad6 => Key::Num6,
        KeyCode::Digit7 | KeyCode::Numpad7 => Key::Num7,
        KeyCode::Digit8 | KeyCode::Numpad8 => Key::Num8,
        KeyCode::Digit9 | KeyCode::Numpad9 => Key::Num9,

        KeyCode::KeyA => Key::A,
        KeyCode::KeyB => Key::B,
        KeyCode::KeyC => Key::C,
        KeyCode::KeyD => Key::D,
        KeyCode::KeyE => Key::E,
        KeyCode::KeyF => Key::F,
        KeyCode::KeyG => Key::G,
        KeyCode::KeyH => Key::H,
        KeyCode::KeyI => Key::I,
        KeyCode::KeyJ => Key::J,
        KeyCode::KeyK => Key::K,
        KeyCode::KeyL => Key::L,
        KeyCode::KeyM => Key::M,
        KeyCode::KeyN => Key::N,
        KeyCode::KeyO => Key::O,
        KeyCode::KeyP => Key::P,
        KeyCode::KeyQ => Key::Q,
        KeyCode::KeyR => Key::R,
        KeyCode::KeyS => Key::S,
        KeyCode::KeyT => Key::T,
        KeyCode::KeyU => Key::U,
        KeyCode::KeyV => Key::V,
        KeyCode::KeyW => Key::W,
        KeyCode::KeyX => Key::X,
        KeyCode::KeyY => Key::Y,
        KeyCode::KeyZ => Key::Z,

        KeyCode::F1 => Key::F1,
        KeyCode::F2 => Key::F2,
        KeyCode::F3 => Key::F3,
        KeyCode::F4 => Key::F4,
        KeyCode::F5 => Key::F5,
        KeyCode::F6 => Key::F6,
        KeyCode::F7 => Key::F7,
        KeyCode::F8 => Key::F8,
        KeyCode::F9 => Key::F9,
        KeyCode::F10 => Key::F10,
        KeyCode::F11 => Key::F11,
        KeyCode::F12 => Key::F12,
        KeyCode::F13 => Key::F13,
        KeyCode::F14 => Key::F14,
        KeyCode::F15 => Key::F15,
        KeyCode::F16 => Key::F16,
        KeyCode::F17 => Key::F17,
        KeyCode::F18 => Key::F18,
        KeyCode::F19 => Key::F19,
        KeyCode::F20 => Key::F20,
        KeyCode::F21 => Key::F21,
        KeyCode::F22 => Key::F22,
        KeyCode::F23 => Key::F23,
        KeyCode::F24 => Key::F24,
        KeyCode::F25 => Key::F25,
        KeyCode::F26 => Key::F26,
        KeyCode::F27 => Key::F27,
        KeyCode::F28 => Key::F28,
        KeyCode::F29 => Key::F29,
        KeyCode::F30 => Key::F30,
        KeyCode::F31 => Key::F31,
        KeyCode::F32 => Key::F32,
        KeyCode::F33 => Key::F33,
        KeyCode::F34 => Key::F34,
        KeyCode::F35 => Key::F35,

        _ => {
            return None;
        }
    })
}

fn translate_cursor(cursor_icon: egui::CursorIcon) -> Option<winit::window::CursorIcon> {
    match cursor_icon {
        egui::CursorIcon::None => None,

        egui::CursorIcon::Alias => Some(winit::window::CursorIcon::Alias),
        egui::CursorIcon::AllScroll => Some(winit::window::CursorIcon::AllScroll),
        egui::CursorIcon::Cell => Some(winit::window::CursorIcon::Cell),
        egui::CursorIcon::ContextMenu => Some(winit::window::CursorIcon::ContextMenu),
        egui::CursorIcon::Copy => Some(winit::window::CursorIcon::Copy),
        egui::CursorIcon::Crosshair => Some(winit::window::CursorIcon::Crosshair),
        egui::CursorIcon::Default => Some(winit::window::CursorIcon::Default),
        egui::CursorIcon::Grab => Some(winit::window::CursorIcon::Grab),
        egui::CursorIcon::Grabbing => Some(winit::window::CursorIcon::Grabbing),
        egui::CursorIcon::Help => Some(winit::window::CursorIcon::Help),
        egui::CursorIcon::Move => Some(winit::window::CursorIcon::Move),
        egui::CursorIcon::NoDrop => Some(winit::window::CursorIcon::NoDrop),
        egui::CursorIcon::NotAllowed => Some(winit::window::CursorIcon::NotAllowed),
        egui::CursorIcon::PointingHand => Some(winit::window::CursorIcon::Pointer),
        egui::CursorIcon::Progress => Some(winit::window::CursorIcon::Progress),

        egui::CursorIcon::ResizeHorizontal => Some(winit::window::CursorIcon::EwResize),
        egui::CursorIcon::ResizeNeSw => Some(winit::window::CursorIcon::NeswResize),
        egui::CursorIcon::ResizeNwSe => Some(winit::window::CursorIcon::NwseResize),
        egui::CursorIcon::ResizeVertical => Some(winit::window::CursorIcon::NsResize),

        egui::CursorIcon::ResizeEast => Some(winit::window::CursorIcon::EResize),
        egui::CursorIcon::ResizeSouthEast => Some(winit::window::CursorIcon::SeResize),
        egui::CursorIcon::ResizeSouth => Some(winit::window::CursorIcon::SResize),
        egui::CursorIcon::ResizeSouthWest => Some(winit::window::CursorIcon::SwResize),
        egui::CursorIcon::ResizeWest => Some(winit::window::CursorIcon::WResize),
        egui::CursorIcon::ResizeNorthWest => Some(winit::window::CursorIcon::NwResize),
        egui::CursorIcon::ResizeNorth => Some(winit::window::CursorIcon::NResize),
        egui::CursorIcon::ResizeNorthEast => Some(winit::window::CursorIcon::NeResize),
        egui::CursorIcon::ResizeColumn => Some(winit::window::CursorIcon::ColResize),
        egui::CursorIcon::ResizeRow => Some(winit::window::CursorIcon::RowResize),

        egui::CursorIcon::Text => Some(winit::window::CursorIcon::Text),
        egui::CursorIcon::VerticalText => Some(winit::window::CursorIcon::VerticalText),
        egui::CursorIcon::Wait => Some(winit::window::CursorIcon::Wait),
        egui::CursorIcon::ZoomIn => Some(winit::window::CursorIcon::ZoomIn),
        egui::CursorIcon::ZoomOut => Some(winit::window::CursorIcon::ZoomOut),
    }
}

// Helpers for egui Viewports
// ---------------------------------------------------------------------------
#[derive(PartialEq, Eq, Hash, Debug)]
pub enum ActionRequested {
    Screenshot(egui::UserData),
    Cut,
    Copy,
    Paste,
}

pub fn process_viewport_commands(
    egui_ctx: &egui::Context,
    info: &mut ViewportInfo,
    commands: impl IntoIterator<Item = ViewportCommand>,
    window: &Window,
    actions_requested: &mut HashSet<ActionRequested>,
) {
    for command in commands {
        process_viewport_command(egui_ctx, window, command, info, actions_requested);
    }
}

fn process_viewport_command(
    egui_ctx: &egui::Context,
    window: &Window,
    command: ViewportCommand,
    info: &mut ViewportInfo,
    actions_requested: &mut HashSet<ActionRequested>,
) {
    profiling::function_scope!();

    use winit::window::ResizeDirection;

    log::trace!("Processing ViewportCommand::{command:?}");

    let pixels_per_point = pixels_per_point(egui_ctx, window);

    match command {
        ViewportCommand::Close => {
            info.events.push(egui::ViewportEvent::Close);
        }
        ViewportCommand::CancelClose => {
            // Need to be handled elsewhere
        }
        ViewportCommand::StartDrag => {
            // If `.has_focus()` is not checked on x11 the input will be permanently taken until the app is killed!
            if window.has_focus() {
                if let Err(err) = window.drag_window() {
                    log::warn!("{command:?}: {err}");
                }
            }
        }
        ViewportCommand::InnerSize(size) => {
            let width_px = pixels_per_point * size.x.max(1.0);
            let height_px = pixels_per_point * size.y.max(1.0);
            let requested_size = PhysicalSize::new(width_px, height_px);
            if let Some(_returned_inner_size) = window.request_inner_size(requested_size) {
                // On platforms where the size is entirely controlled by the user the
                // applied size will be returned immediately, resize event in such case
                // may not be generated.
                // e.g. Linux

                // On platforms where resizing is disallowed by the windowing system, the current
                // inner size is returned immediately, and the user one is ignored.
                // e.g. Android, iOS, …

                // However, comparing the results is prone to numerical errors
                // because the linux backend converts physical to logical and back again.
                // So let's just assume it worked:

                info.inner_rect = inner_rect_in_points(window, pixels_per_point);
                info.outer_rect = outer_rect_in_points(window, pixels_per_point);
            } else {
                // e.g. macOS, Windows
                // The request went to the display system,
                // and the actual size will be delivered later with the [`WindowEvent::Resized`].
            }
        }
        ViewportCommand::BeginResize(direction) => {
            if let Err(err) = window.drag_resize_window(match direction {
                egui::viewport::ResizeDirection::North => ResizeDirection::North,
                egui::viewport::ResizeDirection::South => ResizeDirection::South,
                egui::viewport::ResizeDirection::East => ResizeDirection::East,
                egui::viewport::ResizeDirection::West => ResizeDirection::West,
                egui::viewport::ResizeDirection::NorthEast => ResizeDirection::NorthEast,
                egui::viewport::ResizeDirection::SouthEast => ResizeDirection::SouthEast,
                egui::viewport::ResizeDirection::NorthWest => ResizeDirection::NorthWest,
                egui::viewport::ResizeDirection::SouthWest => ResizeDirection::SouthWest,
            }) {
                log::warn!("{command:?}: {err}");
            }
        }
        ViewportCommand::Title(title) => {
            window.set_title(&title);
        }
        ViewportCommand::Transparent(v) => window.set_transparent(v),
        ViewportCommand::Visible(v) => window.set_visible(v),
        ViewportCommand::OuterPosition(pos) => {
            window.set_outer_position(PhysicalPosition::new(
                pixels_per_point * pos.x,
                pixels_per_point * pos.y,
            ));
        }
        ViewportCommand::MinInnerSize(s) => {
            window.set_min_inner_size((s.is_finite() && s != Vec2::ZERO).then_some(
                PhysicalSize::new(pixels_per_point * s.x, pixels_per_point * s.y),
            ));
        }
        ViewportCommand::MaxInnerSize(s) => {
            window.set_max_inner_size((s.is_finite() && s != Vec2::INFINITY).then_some(
                PhysicalSize::new(pixels_per_point * s.x, pixels_per_point * s.y),
            ));
        }
        ViewportCommand::ResizeIncrements(s) => {
            window.set_resize_increments(
                s.map(|s| PhysicalSize::new(pixels_per_point * s.x, pixels_per_point * s.y)),
            );
        }
        ViewportCommand::Resizable(v) => window.set_resizable(v),
        ViewportCommand::EnableButtons {
            close,
            minimized,
            maximize,
        } => window.set_enabled_buttons(
            if close {
                WindowButtons::CLOSE
            } else {
                WindowButtons::empty()
            } | if minimized {
                WindowButtons::MINIMIZE
            } else {
                WindowButtons::empty()
            } | if maximize {
                WindowButtons::MAXIMIZE
            } else {
                WindowButtons::empty()
            },
        ),
        ViewportCommand::Minimized(v) => {
            window.set_minimized(v);
            info.minimized = Some(v);
        }
        ViewportCommand::Maximized(v) => {
            window.set_maximized(v);
            info.maximized = Some(v);
        }
        ViewportCommand::Fullscreen(v) => {
            window.set_fullscreen(v.then_some(winit::window::Fullscreen::Borderless(None)));
        }
        ViewportCommand::Decorations(v) => window.set_decorations(v),
        ViewportCommand::WindowLevel(l) => window.set_window_level(match l {
            egui::viewport::WindowLevel::AlwaysOnBottom => WindowLevel::AlwaysOnBottom,
            egui::viewport::WindowLevel::AlwaysOnTop => WindowLevel::AlwaysOnTop,
            egui::viewport::WindowLevel::Normal => WindowLevel::Normal,
        }),
        ViewportCommand::Icon(icon) => {
            let winit_icon = icon.and_then(|icon| to_winit_icon(&icon));
            window.set_window_icon(winit_icon);
        }
        ViewportCommand::IMERect(rect) => {
            window.set_ime_cursor_area(
                PhysicalPosition::new(pixels_per_point * rect.min.x, pixels_per_point * rect.min.y),
                PhysicalSize::new(
                    pixels_per_point * rect.size().x,
                    pixels_per_point * rect.size().y,
                ),
            );
        }
        ViewportCommand::IMEAllowed(v) => window.set_ime_allowed(v),
        ViewportCommand::IMEPurpose(p) => window.set_ime_purpose(match p {
            egui::viewport::IMEPurpose::Password => winit::window::ImePurpose::Password,
            egui::viewport::IMEPurpose::Terminal => winit::window::ImePurpose::Terminal,
            egui::viewport::IMEPurpose::Normal => winit::window::ImePurpose::Normal,
        }),
        ViewportCommand::Focus => {
            if !window.has_focus() {
                window.focus_window();
            }
        }
        ViewportCommand::RequestUserAttention(a) => {
            window.request_user_attention(match a {
                egui::UserAttentionType::Reset => None,
                egui::UserAttentionType::Critical => {
                    Some(winit::window::UserAttentionType::Critical)
                }
                egui::UserAttentionType::Informational => {
                    Some(winit::window::UserAttentionType::Informational)
                }
            });
        }
        ViewportCommand::SetTheme(t) => window.set_theme(match t {
            egui::SystemTheme::Light => Some(winit::window::Theme::Light),
            egui::SystemTheme::Dark => Some(winit::window::Theme::Dark),
            egui::SystemTheme::SystemDefault => None,
        }),
        ViewportCommand::ContentProtected(v) => window.set_content_protected(v),
        ViewportCommand::CursorPosition(pos) => {
            if let Err(err) = window.set_cursor_position(PhysicalPosition::new(
                pixels_per_point * pos.x,
                pixels_per_point * pos.y,
            )) {
                log::warn!("{command:?}: {err}");
            }
        }
        ViewportCommand::CursorGrab(o) => {
            if let Err(err) = window.set_cursor_grab(match o {
                egui::viewport::CursorGrab::None => CursorGrabMode::None,
                egui::viewport::CursorGrab::Confined => CursorGrabMode::Confined,
                egui::viewport::CursorGrab::Locked => CursorGrabMode::Locked,
            }) {
                log::warn!("{command:?}: {err}");
            }
        }
        ViewportCommand::CursorVisible(v) => window.set_cursor_visible(v),
        ViewportCommand::MousePassthrough(passthrough) => {
            if let Err(err) = window.set_cursor_hittest(!passthrough) {
                log::warn!("{command:?}: {err}");
            }
        }
        ViewportCommand::Screenshot(user_data) => {
            actions_requested.insert(ActionRequested::Screenshot(user_data));
        }
        ViewportCommand::RequestCut => {
            actions_requested.insert(ActionRequested::Cut);
        }
        ViewportCommand::RequestCopy => {
            actions_requested.insert(ActionRequested::Copy);
        }
        ViewportCommand::RequestPaste => {
            actions_requested.insert(ActionRequested::Paste);
        }
    }
}

/// Build and intitlaize a window.
///
/// Wrapper around `create_winit_window_builder` and `apply_viewport_builder_to_window`.
///
/// # Errors
/// Possible causes of error include denied permission, incompatible system, and lack of memory.
pub fn create_window(
    egui_ctx: &egui::Context,
    event_loop: &ActiveEventLoop,
    viewport_builder: &ViewportBuilder,
) -> Result<Window, winit::error::OsError> {
    profiling::function_scope!();

    let window_attributes =
        create_winit_window_attributes(egui_ctx, event_loop, viewport_builder.clone());
    let window = event_loop.create_window(window_attributes)?;
    apply_viewport_builder_to_window(egui_ctx, &window, viewport_builder);
    Ok(window)
}

pub fn create_winit_window_attributes(
    egui_ctx: &egui::Context,
    event_loop: &ActiveEventLoop,
    viewport_builder: ViewportBuilder,
) -> winit::window::WindowAttributes {
    profiling::function_scope!();

    // We set sizes and positions in egui:s own ui points, which depends on the egui
    // zoom_factor and the native pixels per point, so we need to know that here.
    // We don't know what monitor the window will appear on though, but
    // we'll try to fix that after the window is created in the call to `apply_viewport_builder_to_window`.
    let native_pixels_per_point = event_loop
        .primary_monitor()
        .or_else(|| event_loop.available_monitors().next())
        .map_or_else(
            || {
                log::debug!("Failed to find a monitor - assuming native_pixels_per_point of 1.0");
                1.0
            },
            |m| m.scale_factor() as f32,
        );
    let zoom_factor = egui_ctx.zoom_factor();
    let pixels_per_point = zoom_factor * native_pixels_per_point;

    let ViewportBuilder {
        title,
        position,
        inner_size,
        min_inner_size,
        max_inner_size,
        fullscreen,
        maximized,
        resizable,
        transparent,
        decorations,
        icon,
        active,
        visible,
        close_button,
        minimize_button,
        maximize_button,
        window_level,

        // macOS:
        fullsize_content_view: _fullsize_content_view,
        title_shown: _title_shown,
        titlebar_buttons_shown: _titlebar_buttons_shown,
        titlebar_shown: _titlebar_shown,

        // Windows:
        drag_and_drop: _drag_and_drop,
        taskbar: _taskbar,

        // wayland:
        app_id: _app_id,

        // x11
        window_type: _window_type,

        mouse_passthrough: _, // handled in `apply_viewport_builder_to_window`
        clamp_size_to_monitor_size: _, // Handled in `viewport_builder` in `epi_integration.rs`
    } = viewport_builder;

    let mut window_attributes = winit::window::WindowAttributes::default()
        .with_title(title.unwrap_or_else(|| "egui window".to_owned()))
        .with_transparent(transparent.unwrap_or(false))
        .with_decorations(decorations.unwrap_or(true))
        .with_resizable(resizable.unwrap_or(true))
        .with_visible(visible.unwrap_or(true))
        .with_maximized(if cfg!(target_os = "ios") {
            true
        } else {
            maximized.unwrap_or(false)
        })
        .with_window_level(match window_level.unwrap_or_default() {
            egui::viewport::WindowLevel::AlwaysOnBottom => WindowLevel::AlwaysOnBottom,
            egui::viewport::WindowLevel::AlwaysOnTop => WindowLevel::AlwaysOnTop,
            egui::viewport::WindowLevel::Normal => WindowLevel::Normal,
        })
        .with_fullscreen(
            fullscreen.and_then(|e| e.then_some(winit::window::Fullscreen::Borderless(None))),
        )
        .with_enabled_buttons({
            let mut buttons = WindowButtons::empty();
            if minimize_button.unwrap_or(true) {
                buttons |= WindowButtons::MINIMIZE;
            }
            if maximize_button.unwrap_or(true) {
                buttons |= WindowButtons::MAXIMIZE;
            }
            if close_button.unwrap_or(true) {
                buttons |= WindowButtons::CLOSE;
            }
            buttons
        })
        .with_active(active.unwrap_or(true));

    #[cfg(not(target_os = "ios"))]
    if let Some(size) = inner_size {
        window_attributes = window_attributes.with_inner_size(PhysicalSize::new(
            pixels_per_point * size.x,
            pixels_per_point * size.y,
        ));
    }

    #[cfg(not(target_os = "ios"))]
    if let Some(size) = min_inner_size {
        window_attributes = window_attributes.with_min_inner_size(PhysicalSize::new(
            pixels_per_point * size.x,
            pixels_per_point * size.y,
        ));
    }

    #[cfg(not(target_os = "ios"))]
    if let Some(size) = max_inner_size {
        window_attributes = window_attributes.with_max_inner_size(PhysicalSize::new(
            pixels_per_point * size.x,
            pixels_per_point * size.y,
        ));
    }

    #[cfg(not(target_os = "ios"))]
    if let Some(pos) = position {
        window_attributes = window_attributes.with_position(PhysicalPosition::new(
            pixels_per_point * pos.x,
            pixels_per_point * pos.y,
        ));
    }
    #[cfg(target_os = "ios")]
    {
        // Unused:
        _ = pixels_per_point;
        _ = position;
        _ = inner_size;
        _ = min_inner_size;
        _ = max_inner_size;
    }

    if let Some(icon) = icon {
        let winit_icon = to_winit_icon(&icon);
        window_attributes = window_attributes.with_window_icon(winit_icon);
    }

    #[cfg(all(feature = "wayland", target_os = "linux"))]
    if let Some(app_id) = _app_id {
        use winit::platform::wayland::WindowAttributesExtWayland as _;
        window_attributes = window_attributes.with_name(app_id, "");
    }

    #[cfg(all(feature = "x11", target_os = "linux"))]
    {
        if let Some(window_type) = _window_type {
            use winit::platform::x11::WindowAttributesExtX11 as _;
            use winit::platform::x11::WindowType;
            window_attributes = window_attributes.with_x11_window_type(vec![match window_type {
                egui::X11WindowType::Normal => WindowType::Normal,
                egui::X11WindowType::Utility => WindowType::Utility,
                egui::X11WindowType::Dock => WindowType::Dock,
                egui::X11WindowType::Desktop => WindowType::Desktop,
                egui::X11WindowType::Toolbar => WindowType::Toolbar,
                egui::X11WindowType::Menu => WindowType::Menu,
                egui::X11WindowType::Splash => WindowType::Splash,
                egui::X11WindowType::Dialog => WindowType::Dialog,
                egui::X11WindowType::DropdownMenu => WindowType::DropdownMenu,
                egui::X11WindowType::PopupMenu => WindowType::PopupMenu,
                egui::X11WindowType::Tooltip => WindowType::Tooltip,
                egui::X11WindowType::Notification => WindowType::Notification,
                egui::X11WindowType::Combo => WindowType::Combo,
                egui::X11WindowType::Dnd => WindowType::Dnd,
            }]);
        }
    }

    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::WindowAttributesExtWindows as _;
        if let Some(enable) = _drag_and_drop {
            window_attributes = window_attributes.with_drag_and_drop(enable);
        }
        if let Some(show) = _taskbar {
            window_attributes = window_attributes.with_skip_taskbar(!show);
        }
    }

    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowAttributesExtMacOS as _;
        window_attributes = window_attributes
            .with_title_hidden(!_title_shown.unwrap_or(true))
            .with_titlebar_buttons_hidden(!_titlebar_buttons_shown.unwrap_or(true))
            .with_titlebar_transparent(!_titlebar_shown.unwrap_or(true))
            .with_fullsize_content_view(_fullsize_content_view.unwrap_or(false));
    }

    window_attributes
}

fn to_winit_icon(icon: &egui::IconData) -> Option<winit::window::Icon> {
    if icon.is_empty() {
        None
    } else {
        profiling::function_scope!();
        match winit::window::Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height) {
            Ok(winit_icon) => Some(winit_icon),
            Err(err) => {
                log::warn!("Invalid IconData: {err}");
                None
            }
        }
    }
}

/// Applies what `create_winit_window_builder` couldn't
pub fn apply_viewport_builder_to_window(
    egui_ctx: &egui::Context,
    window: &Window,
    builder: &ViewportBuilder,
) {
    if let Some(mouse_passthrough) = builder.mouse_passthrough {
        if let Err(err) = window.set_cursor_hittest(!mouse_passthrough) {
            log::warn!("set_cursor_hittest failed: {err}");
        }
    }

    {
        // In `create_winit_window_builder` we didn't know
        // on what monitor the window would appear, so we didn't know
        // how to translate egui ui point to native physical pixels.
        // Now we do know:

        let pixels_per_point = pixels_per_point(egui_ctx, window);

        if let Some(size) = builder.inner_size {
            if window
                .request_inner_size(PhysicalSize::new(
                    pixels_per_point * size.x,
                    pixels_per_point * size.y,
                ))
                .is_some()
            {
                log::debug!("Failed to set window size");
            }
        }
        if let Some(size) = builder.min_inner_size {
            window.set_min_inner_size(Some(PhysicalSize::new(
                pixels_per_point * size.x,
                pixels_per_point * size.y,
            )));
        }
        if let Some(size) = builder.max_inner_size {
            window.set_max_inner_size(Some(PhysicalSize::new(
                pixels_per_point * size.x,
                pixels_per_point * size.y,
            )));
        }
        if let Some(pos) = builder.position {
            let pos = PhysicalPosition::new(pixels_per_point * pos.x, pixels_per_point * pos.y);
            window.set_outer_position(pos);
        }
        if let Some(maximized) = builder.maximized {
            window.set_maximized(maximized);
        }
    }
}

// ---------------------------------------------------------------------------

/// Short and fast description of a device event.
/// Useful for logging and profiling.
pub fn short_device_event_description(event: &winit::event::DeviceEvent) -> &'static str {
    use winit::event::DeviceEvent;

    match event {
        DeviceEvent::Added { .. } => "DeviceEvent::Added",
        DeviceEvent::Removed { .. } => "DeviceEvent::Removed",
        DeviceEvent::MouseMotion { .. } => "DeviceEvent::MouseMotion",
        DeviceEvent::MouseWheel { .. } => "DeviceEvent::MouseWheel",
        DeviceEvent::Motion { .. } => "DeviceEvent::Motion",
        DeviceEvent::Button { .. } => "DeviceEvent::Button",
        DeviceEvent::Key { .. } => "DeviceEvent::Key",
    }
}

/// Short and fast description of a window event.
/// Useful for logging and profiling.
pub fn short_window_event_description(event: &winit::event::WindowEvent) -> &'static str {
    use winit::event::WindowEvent;

    match event {
        WindowEvent::ActivationTokenDone { .. } => "WindowEvent::ActivationTokenDone",
        WindowEvent::Resized { .. } => "WindowEvent::Resized",
        WindowEvent::Moved { .. } => "WindowEvent::Moved",
        WindowEvent::CloseRequested { .. } => "WindowEvent::CloseRequested",
        WindowEvent::Destroyed { .. } => "WindowEvent::Destroyed",
        WindowEvent::DroppedFile { .. } => "WindowEvent::DroppedFile",
        WindowEvent::HoveredFile { .. } => "WindowEvent::HoveredFile",
        WindowEvent::HoveredFileCancelled { .. } => "WindowEvent::HoveredFileCancelled",
        WindowEvent::Focused { .. } => "WindowEvent::Focused",
        WindowEvent::KeyboardInput { .. } => "WindowEvent::KeyboardInput",
        WindowEvent::ModifiersChanged { .. } => "WindowEvent::ModifiersChanged",
        WindowEvent::Ime { .. } => "WindowEvent::Ime",
        WindowEvent::CursorMoved { .. } => "WindowEvent::CursorMoved",
        WindowEvent::CursorEntered { .. } => "WindowEvent::CursorEntered",
        WindowEvent::CursorLeft { .. } => "WindowEvent::CursorLeft",
        WindowEvent::MouseWheel { .. } => "WindowEvent::MouseWheel",
        WindowEvent::MouseInput { .. } => "WindowEvent::MouseInput",
        WindowEvent::PinchGesture { .. } => "WindowEvent::PinchGesture",
        WindowEvent::RedrawRequested { .. } => "WindowEvent::RedrawRequested",
        WindowEvent::DoubleTapGesture { .. } => "WindowEvent::DoubleTapGesture",
        WindowEvent::RotationGesture { .. } => "WindowEvent::RotationGesture",
        WindowEvent::TouchpadPressure { .. } => "WindowEvent::TouchpadPressure",
        WindowEvent::AxisMotion { .. } => "WindowEvent::AxisMotion",
        WindowEvent::Touch { .. } => "WindowEvent::Touch",
        WindowEvent::ScaleFactorChanged { .. } => "WindowEvent::ScaleFactorChanged",
        WindowEvent::ThemeChanged { .. } => "WindowEvent::ThemeChanged",
        WindowEvent::Occluded { .. } => "WindowEvent::Occluded",
        WindowEvent::PanGesture { .. } => "WindowEvent::PanGesture",
    }
}
