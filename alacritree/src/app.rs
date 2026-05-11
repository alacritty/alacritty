use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use eframe::CreationContext;
use egui::{Color32, Context, Frame, Margin, RichText, ScrollArea, SidePanel, Stroke};

use crate::bindings::{BindingAction, NamedAction};
use crate::clipboard::{self, Target};
use crate::colors::rgb_to_color32;
use crate::config::Config;
use crate::git_status::{self, ChangeKind, DirtyCounts, FileChange, StatusCache};
use crate::projects::{Project, Worktree};
use crate::session::{Session, SessionId, TermSize};
use crate::state::{self, PersistedProject, PersistedState};
use crate::terminal_view;
use crate::worktree::{self as wt, CreateRequest, Progress};

/// `None` is the home workspace (sessions inherit `$PWD`); `Some` is a worktree path.
type WorkspaceKey = Option<PathBuf>;

/// Channel from notification-worker threads back to the app.  Set once by
/// `AlacritreeApp::new`; each worker reads it to deliver the workspace the
/// user clicked on.  Static because the worker has no other handle to the
/// app and there's only ever one app instance per process.
static NOTIFY_TX: OnceLock<Mutex<Sender<WorkspaceKey>>> = OnceLock::new();

#[derive(Clone, Copy)]
struct Theme {
    terminal_bg: Color32,
    sidebar_bg: Color32,
    sidebar_border: Color32,
    row_hover_bg: Color32,
    row_active_bg: Color32,
    text: Color32,
    text_dim: Color32,
    text_muted: Color32,
    accent: Color32,
    /// "Needs attention" highlight.  Distinct from `accent` ("active
    /// workspace") so the two signals don't read as the same thing.
    attention: Color32,
    /// Logical-pixel size for headings (titles like "Projects", "Git").
    /// `FontConfig::UI_HEADING_RATIO` of the terminal font size.
    font_heading: f32,
    /// Logical-pixel size for normal UI text (rows, captions, button labels).
    /// `FontConfig::UI_NORMAL_RATIO` of the terminal font size — keeps the
    /// chrome secondary to the grid.
    font_normal: f32,
    /// Multiplier applied to hard-coded UI sizes (icons, paddings, modal
    /// widths) so the chrome scales with `font.size`.  Anchored to the
    /// historical 11.25-logical-pixel baseline so unmodified config keeps the
    /// existing layout proportions.
    ui_scale: f32,
}

impl Theme {
    fn from_config(config: &Config) -> Self {
        let terminal_bg = rgb_to_color32(config.palette.bg);
        let sidebar_bg = config.ui.sidebar_background.unwrap_or(terminal_bg);
        let text =
            config.ui.sidebar_foreground.unwrap_or_else(|| rgb_to_color32(config.palette.fg));
        let accent =
            config.ui.sidebar_accent.unwrap_or_else(|| rgb_to_color32(config.palette.normal[4])); // ANSI blue
        let border = config.ui.sidebar_border.unwrap_or_else(|| lighten(sidebar_bg, 0.10));
        Self {
            terminal_bg,
            sidebar_bg,
            sidebar_border: border,
            row_hover_bg: lighten(sidebar_bg, 0.05),
            row_active_bg: lighten(sidebar_bg, 0.10),
            text,
            text_dim: blend_toward(text, sidebar_bg, 0.35),
            text_muted: blend_toward(text, sidebar_bg, 0.55),
            accent,
            attention: rgb_to_color32(config.palette.normal[3]), // ANSI yellow
            font_heading: config.font.ui_heading_px(),
            font_normal: config.font.ui_normal_px(),
            ui_scale: config.font.ui_normal_px() / 11.25,
        }
    }
}

fn lighten(c: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let mix = |x: u8| -> u8 {
        let v = x as f32;
        (v + (255.0 - v) * amount).round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(mix(c.r()), mix(c.g()), mix(c.b()))
}

fn paint_panel_border(ctx: &Context, x: f32, y_range: egui::Rangef, color: Color32) {
    let layer =
        egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("sidebar_border", x.to_bits())));
    ctx.layer_painter(layer).vline(x, y_range, Stroke::new(1.0, color));
}

fn blend_toward(c: Color32, target: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| -> u8 {
        let av = a as f32;
        let bv = b as f32;
        (av + (bv - av) * amount).round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(mix(c.r(), target.r()), mix(c.g(), target.g()), mix(c.b(), target.b()))
}

pub struct AlacritreeApp {
    show_left_sidebar: bool,
    show_right_sidebar: bool,
    sessions: Vec<Session>,
    current_workspace: WorkspaceKey,
    active_session: HashMap<WorkspaceKey, SessionId>,
    projects: Vec<Project>,
    git_status: HashMap<PathBuf, StatusCache>,
    config: Config,
    theme: Theme,
    last_error: Option<String>,
    quit_dialog_open: bool,
    pending_delete: Option<DeleteRequest>,
    pending_create: Option<CreateState>,
    notify_rx: Receiver<WorkspaceKey>,
    /// Shared across sessions; auto-invalidated when cell size changes.
    builtin_glyphs: crate::builtin_font::BuiltinGlyphCache,
}

struct DeleteRequest {
    project_idx: usize,
    worktree_path: PathBuf,
    worktree_name: String,
    branch: Option<String>,
    dirty: DirtyCounts,
}

enum CreateState {
    Prompt { project_idx: usize, branch: String, error: Option<String> },
    Running { project_idx: usize, branch: String, steps: Vec<String>, rx: Receiver<Progress> },
    Done { project_idx: usize, steps: Vec<String>, result: Result<PathBuf, String> },
}

impl AlacritreeApp {
    pub fn new(cc: &CreationContext<'_>, config: Config) -> Self {
        let theme = Theme::from_config(&config);

        crate::fonts::install_terminal_fonts(
            &cc.egui_ctx,
            &config.font.normal,
            &config.font.bold,
            &config.font.italic,
            &config.font.bold_italic,
        );

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = theme.terminal_bg;
        visuals.window_fill = theme.terminal_bg;
        visuals.extreme_bg_color = theme.terminal_bg;
        cc.egui_ctx.set_visuals(visuals);

        // Anchor every text style to the terminal font: titles (unmodified
        // labels) use `Body`/`Heading` at 100% of the grid's text size, and
        // every other UI label (`.small()`, buttons) drops to 80% via
        // `font_normal`.  Spacing knobs scale with the normal-text size so
        // paddings/widths track changes to `font.size`.
        let mut style = (*cc.egui_ctx.style()).clone();
        let scale = theme.ui_scale;
        let heading_px = theme.font_heading;
        let normal_px = theme.font_normal;
        style.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(heading_px));
        style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(heading_px));
        style.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(normal_px));
        style.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(normal_px));
        style.text_styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(normal_px));
        let s = &mut style.spacing;
        s.item_spacing *= scale;
        s.button_padding *= scale;
        s.indent *= scale;
        s.interact_size *= scale;
        s.icon_width *= scale;
        s.icon_width_inner *= scale;
        s.icon_spacing *= scale;
        s.text_edit_width *= scale;
        // egui's debug build paints "Unaligned" labels next to widgets whose
        // edges land on fractional physical pixels.  Our chrome scaling
        // produces non-integer sizes by design (matching `font.size`), so the
        // warning is noise rather than signal — silence it everywhere.
        // `Style::debug` itself is `#[cfg(debug_assertions)]` in egui, so the
        // assignment has to be cfg-gated to keep `--release` compiling.
        #[cfg(debug_assertions)]
        {
            style.debug.show_unaligned = false;
        }
        cc.egui_ctx.set_style(style);

        alacritty_terminal::tty::setup_env();

        let persisted = state::load();
        let projects = persisted
            .projects
            .iter()
            .map(|p| {
                let mut project = Project::discover(p.root.clone());
                project.expanded = p.expanded;
                project
            })
            .collect();

        let (notify_tx, notify_rx) = mpsc::channel();
        // `set` may fail only if a previous instance already initialized the
        // static (e.g. tests).  In that case the old sender points at a dead
        // app, so overwriting via `Mutex` would be ideal — but since we only
        // ever spawn one app per process, ignoring the error is fine.
        let _ = NOTIFY_TX.set(Mutex::new(notify_tx));

        let mut app = Self {
            show_left_sidebar: persisted.show_left_sidebar,
            show_right_sidebar: persisted.show_right_sidebar,
            sessions: Vec::new(),
            current_workspace: None,
            active_session: HashMap::new(),
            projects,
            git_status: HashMap::new(),
            config,
            theme,
            last_error: None,
            quit_dialog_open: false,
            pending_delete: None,
            pending_create: None,
            notify_rx,
            builtin_glyphs: crate::builtin_font::BuiltinGlyphCache::new(),
        };

        if let Err(e) = app.spawn_session(&cc.egui_ctx, None) {
            app.last_error = Some(format!("failed to spawn shell: {e}"));
        }

        app
    }

    fn persist(&self) {
        let state = PersistedState {
            projects: self
                .projects
                .iter()
                .map(|p| PersistedProject { root: p.root.clone(), expanded: p.expanded })
                .collect(),
            show_left_sidebar: self.show_left_sidebar,
            show_right_sidebar: self.show_right_sidebar,
        };
        state::save(&state);
    }

    fn spawn_session(
        &mut self,
        ctx: &Context,
        working_directory: WorkspaceKey,
    ) -> std::io::Result<SessionId> {
        let session = Session::spawn(
            ctx.clone(),
            &self.config,
            working_directory.clone(),
            TermSize::new(80, 24),
            (8.0, 16.0),
        )?;
        let id = session.id;
        self.sessions.push(session);
        self.active_session.insert(working_directory, id);
        Ok(id)
    }

    fn activate_worktree(&mut self, ctx: &Context, path: &Path) {
        self.current_workspace = Some(path.to_path_buf());
        self.ensure_active_session(ctx);
    }

    fn activate_home(&mut self, ctx: &Context) {
        self.current_workspace = None;
        self.ensure_active_session(ctx);
    }

    fn ensure_active_session(&mut self, ctx: &Context) {
        if self.active_session_index().is_some() {
            return;
        }
        let ws_idx = self.workspace_session_indices(&self.current_workspace);
        if let Some(&idx) = ws_idx.first() {
            let id = self.sessions[idx].id;
            self.active_session.insert(self.current_workspace.clone(), id);
            return;
        }
        if let Err(e) = self.spawn_session(ctx, self.current_workspace.clone()) {
            self.last_error = Some(format!("failed to spawn shell: {e}"));
        }
    }

    fn close_session(&mut self, id: SessionId) {
        let Some(idx) = self.sessions.iter().position(|s| s.id == id) else {
            return;
        };
        let workspace = self.sessions[idx].working_directory.clone();
        self.sessions.remove(idx);

        if self.active_session.get(&workspace).copied() == Some(id) {
            let fallback =
                self.sessions.iter().find(|s| s.working_directory == workspace).map(|s| s.id);
            match fallback {
                Some(new_id) => {
                    self.active_session.insert(workspace, new_id);
                },
                None => {
                    self.active_session.remove(&workspace);
                },
            }
        }
    }

    fn workspace_session_indices(&self, ws: &WorkspaceKey) -> Vec<usize> {
        self.sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| s.working_directory == *ws)
            .map(|(i, _)| i)
            .collect()
    }

    fn current_session_indices(&self) -> Vec<usize> {
        self.workspace_session_indices(&self.current_workspace)
    }

    fn active_session_index(&self) -> Option<usize> {
        let id = self.active_session.get(&self.current_workspace).copied()?;
        self.sessions.iter().position(|s| s.id == id)
    }

    fn set_active_in_current_workspace(&mut self, id: SessionId) {
        self.active_session.insert(self.current_workspace.clone(), id);
    }

    fn cycle_tabs(&mut self, delta: i32) {
        let indices = self.current_session_indices();
        if indices.len() < 2 {
            return;
        }
        let current = self.active_session_index().unwrap_or(indices[0]);
        let pos = indices.iter().position(|&i| i == current).unwrap_or(0);
        let len = indices.len() as i32;
        let new_pos = ((pos as i32 + delta).rem_euclid(len)) as usize;
        let id = self.sessions[indices[new_pos]].id;
        self.set_active_in_current_workspace(id);
    }

    fn cycle_workspaces(&mut self, ctx: &Context, delta: i32) {
        let order = self.workspace_order();
        if order.len() < 2 {
            return;
        }
        let cur_pos = order.iter().position(|w| *w == self.current_workspace).unwrap_or(0);
        let len = order.len() as i32;
        let new_pos = ((cur_pos as i32 + delta).rem_euclid(len)) as usize;
        match &order[new_pos] {
            None => self.activate_home(ctx),
            Some(p) => {
                let path = p.clone();
                self.activate_worktree(ctx, &path);
            },
        }
    }

    fn workspace_order(&self) -> Vec<WorkspaceKey> {
        let mut order: Vec<WorkspaceKey> = vec![None];
        for project in &self.projects {
            for wt in &project.worktrees {
                order.push(Some(wt.path.clone()));
            }
        }
        order
    }

    fn add_project_via_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            if !self.projects.iter().any(|p| p.root == path) {
                self.projects.push(Project::discover(path));
                self.persist();
            }
        }
    }

    fn is_modal_open(&self) -> bool {
        self.quit_dialog_open || self.pending_delete.is_some() || self.pending_create.is_some()
    }

    fn handle_shortcuts(&mut self, ctx: &Context) {
        let mut sidebars_changed = false;
        let mut cycle_tabs_delta: Option<i32> = None;
        let mut cycle_ws_delta: Option<i32> = None;
        let mut quit_requested = false;
        let mut add_project_requested = false;
        ctx.input_mut(|i| {
            if consume_exact(i, egui::Modifiers::CTRL, egui::Key::B) {
                self.show_left_sidebar = !self.show_left_sidebar;
                sidebars_changed = true;
            }
            if consume_exact(i, egui::Modifiers::CTRL, egui::Key::G) {
                self.show_right_sidebar = !self.show_right_sidebar;
                sidebars_changed = true;
            }
            // Ctrl+Shift+Tab must be checked before Ctrl+Tab — even with exact
            // matching, leaving them in the opposite order works, but ordering
            // forward→backward keeps the "modifier-richer wins" intent obvious.
            if consume_exact(i, egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Tab) {
                cycle_tabs_delta = Some(-1);
            }
            if consume_exact(i, egui::Modifiers::CTRL, egui::Key::Tab) {
                cycle_tabs_delta = Some(1);
            }
            if consume_exact(i, egui::Modifiers::ALT, egui::Key::ArrowRight) {
                cycle_ws_delta = Some(1);
            }
            if consume_exact(i, egui::Modifiers::ALT, egui::Key::ArrowLeft) {
                cycle_ws_delta = Some(-1);
            }
            if consume_exact(i, egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::O) {
                add_project_requested = true;
            }
            if consume_exact(i, egui::Modifiers::CTRL, egui::Key::Q) {
                quit_requested = true;
            }
        });

        // Split out so spawn_session can take &mut self without tripping the borrow.
        let ctrl_t = ctx.input_mut(|i| consume_exact(i, egui::Modifiers::CTRL, egui::Key::T));
        if ctrl_t {
            let ws = self.current_workspace.clone();
            if let Err(e) = self.spawn_session(ctx, ws) {
                self.last_error = Some(format!("failed to spawn shell: {e}"));
            }
        }
        if add_project_requested {
            self.add_project_via_dialog();
        }
        if let Some(d) = cycle_tabs_delta {
            self.cycle_tabs(d);
        }
        if let Some(d) = cycle_ws_delta {
            self.cycle_workspaces(ctx, d);
        }
        if quit_requested {
            self.quit_dialog_open = true;
        }
        if sidebars_changed {
            self.persist();
        }

        // After built-in shortcuts (so we don't shadow them) but before the
        // terminal sees raw events (so a binding wins over plain text input).
        self.dispatch_user_bindings(ctx);
    }

    fn dispatch_user_bindings(&mut self, ctx: &Context) {
        if self.config.bindings.is_empty() {
            return;
        }
        let actions: Vec<BindingAction> = ctx.input_mut(|i| {
            let mut actions = Vec::new();
            i.events.retain(|ev| {
                if let egui::Event::Key { key, pressed: true, modifiers, .. } = ev {
                    if let Some(action) =
                        crate::bindings::matches(&self.config.bindings, *key, *modifiers)
                    {
                        actions.push(action.clone());
                        return false;
                    }
                }
                true
            });
            actions
        });
        for action in actions {
            self.dispatch_action(ctx, action);
        }
    }

    fn dispatch_action(&mut self, ctx: &Context, action: BindingAction) {
        match action {
            BindingAction::Chars(bytes) => {
                if let Some(idx) = self.active_session_index() {
                    self.sessions[idx].write(bytes);
                }
            },
            BindingAction::Named(NamedAction::Paste) => {
                if let Some(text) = clipboard::read(Target::Clipboard) {
                    if let Some(idx) = self.active_session_index() {
                        self.sessions[idx].write(text.into_bytes());
                    }
                }
            },
            BindingAction::Named(NamedAction::PasteSelection) => {
                if let Some(text) = clipboard::read(Target::Primary) {
                    if let Some(idx) = self.active_session_index() {
                        self.sessions[idx].write(text.into_bytes());
                    }
                }
            },
            BindingAction::Named(NamedAction::Copy) => {
                if let Some(idx) = self.active_session_index() {
                    if let Some(text) = self.sessions[idx].term.lock().selection_to_string() {
                        if !text.is_empty() {
                            clipboard::write(Target::Clipboard, &text);
                        }
                    }
                }
            },
            BindingAction::Named(NamedAction::SpawnNewInstance) => {
                let ws = self.current_workspace.clone();
                if let Err(e) = self.spawn_session(ctx, ws) {
                    self.last_error = Some(format!("failed to spawn shell: {e}"));
                }
            },
            BindingAction::Named(NamedAction::Quit) => {
                self.quit_dialog_open = true;
            },
            BindingAction::Named(NamedAction::ClearHistory) => {
                use alacritty_terminal::vte::ansi::{ClearMode, Handler};
                if let Some(idx) = self.active_session_index() {
                    self.sessions[idx].term.lock().clear_screen(ClearMode::Saved);
                }
            },
            BindingAction::Named(NamedAction::ToggleFullscreen) => {
                let on = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!on));
            },
            BindingAction::Named(NamedAction::ToggleMaximized) => {
                let on = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!on));
            },
            BindingAction::Named(NamedAction::Minimize) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            },
            BindingAction::Named(NamedAction::SelectNextTab) => self.cycle_tabs(1),
            BindingAction::Named(NamedAction::SelectPreviousTab) => self.cycle_tabs(-1),
            BindingAction::Named(NamedAction::SelectTab(n)) => self.select_tab(n),
            BindingAction::Named(NamedAction::SelectLastTab) => self.select_last_tab(),
            BindingAction::Named(NamedAction::NoOp) => {},
            BindingAction::Named(other) => {
                self.dispatch_scroll_or_other(other);
            },
            BindingAction::Unsupported(name) => {
                log::debug!("unsupported keyboard binding action: {name}");
            },
        }
    }

    fn dispatch_scroll_or_other(&mut self, action: NamedAction) {
        use alacritty_terminal::grid::{Dimensions, Scroll};
        let Some(idx) = self.active_session_index() else {
            return;
        };
        let session = &mut self.sessions[idx];
        let mut term = session.term.lock();
        let lines_per_page = term.grid().screen_lines() as i32;
        let scroll = match action {
            NamedAction::ScrollLineUp => Some(Scroll::Delta(1)),
            NamedAction::ScrollLineDown => Some(Scroll::Delta(-1)),
            NamedAction::ScrollHalfPageUp => Some(Scroll::Delta(lines_per_page / 2)),
            NamedAction::ScrollHalfPageDown => Some(Scroll::Delta(-(lines_per_page / 2))),
            NamedAction::ScrollPageUp => Some(Scroll::PageUp),
            NamedAction::ScrollPageDown => Some(Scroll::PageDown),
            NamedAction::ScrollToTop => Some(Scroll::Top),
            NamedAction::ScrollToBottom => Some(Scroll::Bottom),
            _ => None,
        };
        if let Some(s) = scroll {
            term.scroll_display(s);
        }
    }

    fn select_tab(&mut self, n: u8) {
        if n == 0 {
            return;
        }
        let indices = self.current_session_indices();
        let Some(&session_idx) = indices.get((n - 1) as usize) else {
            return;
        };
        let id = self.sessions[session_idx].id;
        self.set_active_in_current_workspace(id);
    }

    fn select_last_tab(&mut self) {
        let indices = self.current_session_indices();
        let Some(&session_idx) = indices.last() else {
            return;
        };
        let id = self.sessions[session_idx].id;
        self.set_active_in_current_workspace(id);
    }

    fn show_tab_strip(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let indices = self.current_session_indices();
        if indices.len() < 2 {
            ui.add_space(2.0);
            return;
        }
        let active_idx = self.active_session_index();

        // Reserve a 2px-tall strip across the full width of the terminal pane.
        let strip_height = 2.0;
        let gap = 4.0;
        let avail = ui.available_width();
        let segment_width =
            ((avail - gap * (indices.len() as f32 - 1.0)) / indices.len() as f32).max(1.0);
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(avail, strip_height + 2.0), egui::Sense::hover());

        let mut activate: Option<SessionId> = None;
        for (i, &session_idx) in indices.iter().enumerate() {
            let x0 = rect.min.x + i as f32 * (segment_width + gap);
            let seg_rect = egui::Rect::from_min_size(
                egui::pos2(x0, rect.min.y + 1.0),
                egui::vec2(segment_width, strip_height),
            );
            let is_active = active_idx == Some(session_idx);
            // 2px is too small to reliably click — expand the hit zone vertically.
            let click_rect = seg_rect.expand2(egui::vec2(0.0, 4.0));
            let id = ui.id().with(("tab_strip", self.sessions[session_idx].id));
            let resp = ui.interact(click_rect, id, egui::Sense::click());
            // Attention wins over the active/inactive shading so a bell from a
            // non-active tab pulls the eye even when another tab is selected.
            let color = if self.sessions[session_idx].needs_attention {
                theme.attention
            } else if is_active {
                theme.text
            } else if resp.hovered() {
                theme.text_dim
            } else {
                theme.text_muted
            };
            ui.painter().rect_filled(seg_rect, 0.0, color);
            if resp.clicked() {
                activate = Some(self.sessions[session_idx].id);
            }
            if resp.hovered() {
                resp.on_hover_text(&self.sessions[session_idx].title);
            }
        }

        if let Some(id) = activate {
            self.set_active_in_current_workspace(id);
        }
    }

    fn show_project_sidebar(&mut self, ctx: &Context, panel_frame: Frame) -> egui::Rect {
        let activate_request: std::cell::Cell<Option<PathBuf>> = std::cell::Cell::new(None);
        let delete_request: std::cell::Cell<Option<DeleteRequest>> = std::cell::Cell::new(None);
        let create_request: std::cell::Cell<Option<usize>> = std::cell::Cell::new(None);
        let mut add_project_clicked = false;
        let mut refresh_idx: Option<usize> = None;
        let mut remove_idx: Option<usize> = None;
        let mut expand_toggled = false;
        let mut home_clicked = false;
        let theme = self.theme;

        // Snapshot attention + agent-glyph state up-front so the `iter_mut`
        // over projects below isn't blocked from calling back into `&self`
        // helpers.
        let home_attention = self.workspace_needs_attention(&None);
        let home_agent_glyph = self.workspace_agent_glyph(&None);
        let project_attention: Vec<bool> =
            self.projects.iter().map(|p| self.project_needs_attention(p)).collect();
        let worktree_attention: Vec<Vec<bool>> = self
            .projects
            .iter()
            .map(|p| {
                p.worktrees
                    .iter()
                    .map(|wt| self.workspace_needs_attention(&Some(wt.path.clone())))
                    .collect()
            })
            .collect();
        let worktree_agent: Vec<Vec<Option<char>>> = self
            .projects
            .iter()
            .map(|p| {
                p.worktrees
                    .iter()
                    .map(|wt| self.workspace_agent_glyph(&Some(wt.path.clone())))
                    .collect()
            })
            .collect();

        let panel_resp = SidePanel::left("left_sidebar")
            .resizable(true)
            .default_width(240.0 * theme.ui_scale)
            .min_width(180.0 * theme.ui_scale)
            .frame(panel_frame)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Projects").color(theme.text).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, "+", theme.text_dim, &theme)
                            .on_hover_text("add project")
                            .clicked()
                        {
                            add_project_clicked = true;
                        }
                    });
                });
                ui.separator();

                ScrollArea::vertical().show(ui, |ui| {
                    if home_row(
                        ui,
                        self.current_workspace.is_none(),
                        home_attention,
                        home_agent_glyph,
                        &theme,
                    )
                    .clicked()
                    {
                        home_clicked = true;
                    }
                    ui.add_space(2.0);

                    if self.projects.is_empty() {
                        ui.label(
                            RichText::new("Click + to add a project.")
                                .color(theme.text_dim)
                                .small(),
                        );
                        ui.add_space(4.0);
                        ui.label(RichText::new("Ctrl+B to toggle").small().color(theme.text_muted));
                    }

                    for (idx, project) in self.projects.iter_mut().enumerate() {
                        let proj_attention = project_attention.get(idx).copied().unwrap_or(false);
                        // Bubble attention up to the project row only when the
                        // project is collapsed — once expanded, the actual
                        // worktree rows already show the dot, and doubling it
                        // on the parent reads as noise.
                        let show_proj_dot = proj_attention && !project.expanded;
                        row_with_trailing(
                            ui,
                            |ui| {
                                let arrow = if project.expanded { "▾" } else { "▸" };
                                if icon_button(ui, arrow, theme.text_dim, &theme).clicked() {
                                    project.expanded = !project.expanded;
                                    expand_toggled = true;
                                }
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&project.name)
                                            .color(theme.text)
                                            .strong()
                                            .small(),
                                    )
                                    .truncate(),
                                );
                            },
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                if icon_button(ui, "×", theme.text_muted, &theme)
                                    .on_hover_text("remove from sidebar")
                                    .clicked()
                                {
                                    remove_idx = Some(idx);
                                }
                                if icon_button(ui, "↻", theme.text_muted, &theme)
                                    .on_hover_text("refresh worktrees")
                                    .clicked()
                                {
                                    refresh_idx = Some(idx);
                                }
                                if icon_button(ui, "+", theme.text_muted, &theme)
                                    .on_hover_text("create new worktree")
                                    .clicked()
                                {
                                    create_request.set(Some(idx));
                                }
                                if show_proj_dot {
                                    attention_dot(ui, &theme);
                                }
                            },
                        );

                        if project.expanded {
                            for (wt_idx, wt) in project.worktrees.iter().enumerate() {
                                let is_active = self.current_workspace.as_deref() == Some(&wt.path);
                                let wt_attention = worktree_attention
                                    .get(idx)
                                    .and_then(|v| v.get(wt_idx))
                                    .copied()
                                    .unwrap_or(false);
                                let wt_glyph = worktree_agent
                                    .get(idx)
                                    .and_then(|v| v.get(wt_idx))
                                    .copied()
                                    .unwrap_or(None);
                                let action =
                                    worktree_row(ui, wt, is_active, wt_attention, wt_glyph, &theme);
                                if action.activate {
                                    activate_request.set(Some(wt.path.clone()));
                                }
                                if action.delete {
                                    delete_request.set(Some(DeleteRequest {
                                        project_idx: idx,
                                        worktree_path: wt.path.clone(),
                                        worktree_name: wt.name.clone(),
                                        branch: wt.branch.clone(),
                                        dirty: git_status::dirty_counts(&wt.path),
                                    }));
                                }
                            }
                            ui.add_space(4.0);
                        }
                    }
                });
            });

        if add_project_clicked {
            self.add_project_via_dialog();
        }
        if let Some(idx) = refresh_idx {
            self.projects[idx].refresh();
        }
        if let Some(idx) = remove_idx {
            self.projects.remove(idx);
            self.persist();
        }
        if expand_toggled {
            self.persist();
        }
        if home_clicked {
            self.activate_home(ctx);
        }
        if let Some(path) = activate_request.take() {
            self.activate_worktree(ctx, &path);
        }
        if let Some(req) = delete_request.take() {
            self.pending_delete = Some(req);
        }
        if let Some(idx) = create_request.take() {
            self.pending_create =
                Some(CreateState::Prompt { project_idx: idx, branch: String::new(), error: None });
        }
        panel_resp.response.rect
    }

    fn active_session_path(&self) -> Option<PathBuf> {
        self.current_workspace.clone()
    }

    fn project_default_branch_for(&self, path: &Path) -> Option<String> {
        for project in &self.projects {
            for wt in &project.worktrees {
                if wt.path == path {
                    return project.default_branch.clone();
                }
            }
        }
        None
    }

    fn show_git_sidebar(&mut self, ctx: &Context, panel_frame: Frame) -> egui::Rect {
        let theme = self.theme;
        let palette = self.config.palette.clone();
        let panel_resp = SidePanel::right("right_sidebar")
            .resizable(true)
            .default_width(300.0 * theme.ui_scale)
            .min_width(220.0 * theme.ui_scale)
            .frame(panel_frame)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Git").color(theme.text).strong());
                });
                ui.separator();

                let path = match self.active_session_path() {
                    Some(p) => p,
                    None => {
                        ScrollArea::vertical().show(ui, |ui| {
                            ui.label(
                                RichText::new("Open a worktree from the left sidebar.")
                                    .color(theme.text_dim)
                                    .small(),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new("Ctrl+G to toggle").small().color(theme.text_muted),
                            );
                        });
                        return;
                    },
                };

                let default_branch = self.project_default_branch_for(&path);
                let cache = self
                    .git_status
                    .entry(path.clone())
                    .or_insert_with(|| StatusCache::new(path.clone()));

                let mut status = cache.get().clone();
                if let Some(branch) = default_branch.as_deref() {
                    if status.default_branch.as_deref() != Some(branch) {
                        cache.force_refresh(Some(branch));
                        status = cache.get().clone();
                    }
                }

                ScrollArea::vertical().show(ui, |ui| {
                    if let Some(err) = &status.error {
                        ui.label(
                            RichText::new(err).color(rgb_to_color32(palette.normal[1])).small(),
                        );
                        return;
                    }

                    ui.add(
                        egui::Label::new(
                            RichText::new(path.display().to_string())
                                .color(theme.text_muted)
                                .small(),
                        )
                        .truncate(),
                    );
                    if let Some(branch) = &status.branch {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("on").color(theme.text_muted).small());
                            ui.add(
                                egui::Label::new(
                                    RichText::new(branch).color(theme.accent).small().strong(),
                                )
                                .truncate(),
                            );
                            if let Some(default) = &status.default_branch {
                                if Some(branch) != Some(default) {
                                    ui.label(RichText::new("vs").color(theme.text_muted).small());
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(default).color(theme.text_dim).small(),
                                        )
                                        .truncate(),
                                    );
                                }
                            }
                        });
                    }
                    ui.add_space(10.0);

                    section(ui, &theme, "Staged", status.staged.len(), |ui| {
                        for f in &status.staged {
                            file_row(ui, f, &theme, &palette);
                        }
                    });

                    section(ui, &theme, "Unstaged", status.unstaged.len(), |ui| {
                        for f in &status.unstaged {
                            file_row(ui, f, &theme, &palette);
                        }
                    });

                    if !status.branch_diff.is_empty() {
                        let label = match (&status.default_branch, &status.default_branch_resolved)
                        {
                            (Some(b), _) => format!("Changes vs {}", b),
                            _ => "Changes vs default".to_string(),
                        };
                        let added = rgb_to_color32(palette.normal[2]);
                        let removed = rgb_to_color32(palette.normal[1]);
                        section(ui, &theme, &label, status.branch_diff.len(), |ui| {
                            for stat in &status.branch_diff {
                                row_with_trailing(
                                    ui,
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&stat.path)
                                                    .color(theme.text_dim)
                                                    .monospace()
                                                    .small(),
                                            )
                                            .truncate(),
                                        );
                                    },
                                    |ui| {
                                        if stat.deletions > 0 {
                                            ui.label(
                                                RichText::new(format!("-{}", stat.deletions))
                                                    .color(removed)
                                                    .small()
                                                    .monospace(),
                                            );
                                        }
                                        if stat.additions > 0 {
                                            ui.label(
                                                RichText::new(format!("+{}", stat.additions))
                                                    .color(added)
                                                    .small()
                                                    .monospace(),
                                            );
                                        }
                                    },
                                );
                            }
                        });
                    }
                });
            });
        panel_resp.response.rect
    }
}

fn dirty_warning(counts: &DirtyCounts) -> Option<String> {
    if !counts.is_dirty() {
        return None;
    }
    let mut parts = Vec::new();
    if counts.staged > 0 {
        parts.push(format!("{} staged", counts.staged));
    }
    if counts.modified > 0 {
        parts.push(format!("{} modified", counts.modified));
    }
    if counts.untracked > 0 {
        parts.push(format!("{} untracked", counts.untracked));
    }
    Some(format!(
        "Working tree has {} file(s) — they will be discarded with --force.",
        parts.join(", ")
    ))
}

fn modal_frame(theme: &Theme) -> Frame {
    let s = theme.ui_scale;
    let pad_x = (16.0 * s).round() as i8;
    let pad_y = (12.0 * s).round() as i8;
    Frame::default()
        .fill(theme.sidebar_bg)
        .stroke(Stroke::new(1.0, theme.sidebar_border))
        .inner_margin(Margin { left: pad_x, right: pad_x, top: pad_y, bottom: pad_y })
}

/// `InputState::consume_shortcut` matches modifiers as a *subset*: `Ctrl+G`
/// fires on `Ctrl+Shift+G`, `Ctrl+Tab` fires on `Ctrl+Shift+Tab`, and the
/// stronger binding never gets a chance to consume the event.  Use exact
/// modifier matching so each shortcut only triggers on its own combination.
fn consume_exact(input: &mut egui::InputState, mods: egui::Modifiers, key: egui::Key) -> bool {
    let mut hit = false;
    input.events.retain(|ev| {
        if let egui::Event::Key { key: k, pressed: true, modifiers, .. } = ev {
            if *k == key && modifiers.matches_exact(mods) {
                hit = true;
                return false;
            }
        }
        true
    });
    hit
}

fn consume_modal_keys(ctx: &Context) -> (bool, bool) {
    ctx.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
        )
    })
}

/// Move focus to `id` if no widget currently has it — gives the modal's
/// primary control focus on open without stealing it from the user later.
fn focus_default(ctx: &Context, id: egui::Id) {
    let has_focus = ctx.memory(|m| m.focused().is_some());
    if !has_focus {
        ctx.memory_mut(|m| m.request_focus(id));
    }
}

/// Render a collapsed-when-empty git section.
///
/// Empty sections are skipped entirely — a placeholder glyph for "no files
/// here" added visual noise without communicating anything the count badge
/// didn't already say.
fn section<R>(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    count: usize,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) {
    if count == 0 {
        return;
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).color(theme.text).strong().small());
        ui.label(RichText::new(format!("{count}")).color(theme.text_muted).small());
    });
    ui.add_space(2.0);
    add_contents(ui);
    ui.add_space(10.0);
}

fn file_row(
    ui: &mut egui::Ui,
    change: &FileChange,
    theme: &Theme,
    palette: &crate::config::Palette,
) {
    ui.horizontal(|ui| {
        let color = match change.kind {
            ChangeKind::Added | ChangeKind::Untracked => rgb_to_color32(palette.normal[2]),
            ChangeKind::Modified => rgb_to_color32(palette.normal[3]),
            ChangeKind::Deleted => rgb_to_color32(palette.normal[1]),
            ChangeKind::Renamed => rgb_to_color32(palette.normal[4]),
            ChangeKind::Conflicted => rgb_to_color32(palette.bright[1]),
        };
        ui.label(RichText::new(change.kind.glyph()).color(color).monospace().small());
        ui.add(
            egui::Label::new(RichText::new(&change.path).color(theme.text_dim).monospace().small())
                .truncate(),
        );
    });
}

/// Lay out a row whose `trailing` widgets pin to the right edge while `leading`
/// fills the remaining width — so a `Label::truncate()` inside `leading` knows
/// exactly how much space it has and ellipsizes cleanly when the panel is narrow.
///
/// The row is pre-sized to `interact_size.y` (mirroring `Ui::horizontal`'s own
/// internals) so it doesn't claim the parent's full remaining height when nested
/// in a vertical layout — without this, `Align::Center` would push the row's
/// content to the middle of the column and leave a giant gap before the next row.
/// Frameless, fixed-footprint icon button. Painter-drawn rather than a
/// `Button` because `Button` lays text out from the top-left of its rect, so
/// glyphs of different intrinsic heights (e.g. `+` vs `↻`) end up on different
/// baselines. `painter.text` with `CENTER_CENTER` centers the galley in the
/// rect, giving real grid alignment.
/// Painted (rather than `RichText("●")`) so its size is independent of font
/// metrics — `RichText("●")` renders inconsistently across fallback fonts.
fn attention_dot(ui: &mut egui::Ui, theme: &Theme) {
    let s = theme.ui_scale;
    let size = egui::vec2(10.0 * s, 14.0 * s);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let radius = 3.0 * s;
    ui.painter().circle_filled(rect.center(), radius, theme.attention);
}

/// Priority: attention dot > agent glyph (animated by the agent's own title
/// updates) > the row's default marker.
fn paint_row_status_icon(
    ui: &mut egui::Ui,
    theme: &Theme,
    attention: bool,
    agent_glyph: Option<char>,
    default_glyph: &str,
    is_active: bool,
) {
    if attention {
        attention_dot(ui, theme);
        return;
    }
    let s = theme.ui_scale;
    let (glyph, color) = match agent_glyph {
        Some(g) => (g.to_string(), if is_active { theme.accent } else { theme.text }),
        None => {
            (default_glyph.to_string(), if is_active { theme.accent } else { theme.text_muted })
        },
    };
    ui.label(RichText::new(glyph).color(color).size(10.0 * s));
}

fn icon_button(ui: &mut egui::Ui, glyph: &str, color: Color32, theme: &Theme) -> egui::Response {
    let s = theme.ui_scale;
    let size = egui::vec2(16.0 * s, 16.0 * s);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let painted = if resp.hovered() {
        Color32::from_rgb(
            color.r().saturating_add(40),
            color.g().saturating_add(40),
            color.b().saturating_add(40),
        )
    } else {
        color
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(12.0 * s),
        painted,
    );
    resp
}

fn row_with_trailing<L, T>(ui: &mut egui::Ui, leading: L, trailing: T)
where
    L: FnOnce(&mut egui::Ui),
    T: FnOnce(&mut egui::Ui),
{
    let row_size = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(row_size, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        trailing(ui);
        let remaining = ui.available_width();
        if remaining <= 0.0 {
            return;
        }
        let row_h = ui.available_height();
        ui.allocate_ui_with_layout(
            egui::vec2(remaining, row_h),
            egui::Layout::left_to_right(egui::Align::Center),
            leading,
        );
    });
}

fn home_row(
    ui: &mut egui::Ui,
    is_active: bool,
    attention: bool,
    agent_glyph: Option<char>,
    theme: &Theme,
) -> egui::Response {
    // Reserve a slot *before* the labels so the hover bg paints beneath them.
    let bg_idx = ui.painter().add(egui::Shape::Noop);
    let panel_x = ui.max_rect().x_range();

    let frame = Frame::default().inner_margin(Margin { left: 6, right: 6, top: 3, bottom: 3 });
    let resp = frame
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                paint_row_status_icon(ui, theme, attention, agent_glyph, "⌂", is_active);
                ui.label(
                    RichText::new("Home")
                        .color(if is_active { theme.text } else { theme.text_dim })
                        .strong()
                        .small(),
                );
            });
        })
        .response
        .interact(egui::Sense::click());

    let bg = if is_active {
        theme.row_active_bg
    } else if resp.hovered() {
        theme.row_hover_bg
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        let rect = egui::Rect::from_x_y_ranges(panel_x, resp.rect.y_range());
        ui.painter().set(bg_idx, egui::Shape::rect_filled(rect, 0.0, bg));
    }
    resp
}

struct WorktreeAction {
    activate: bool,
    delete: bool,
}

fn worktree_row(
    ui: &mut egui::Ui,
    wt: &Worktree,
    is_active: bool,
    attention: bool,
    agent_glyph: Option<char>,
    theme: &Theme,
) -> WorktreeAction {
    // Reserve a slot *before* the labels so the hover bg paints beneath them.
    let bg_idx = ui.painter().add(egui::Shape::Noop);
    let panel_x = ui.max_rect().x_range();

    let mut delete_clicked = false;
    let mut delete_rect: Option<egui::Rect> = None;
    // right: 0 keeps the worktree `×` at the same x as the project row's `×`,
    // which has no frame margin and sits flush against the panel's outer padding.
    let frame = Frame::default().inner_margin(Margin { left: 16, right: 0, top: 3, bottom: 3 });
    let resp = frame
        .show(ui, |ui| {
            let default_icon = if wt.is_main { "●" } else { "○" };
            let name_color = if is_active { theme.text } else { theme.text_dim };
            row_with_trailing(
                ui,
                |ui| {
                    paint_row_status_icon(
                        ui,
                        theme,
                        attention,
                        agent_glyph,
                        default_icon,
                        is_active,
                    );
                    ui.add(
                        egui::Label::new(RichText::new(&wt.name).color(name_color).small())
                            .truncate(),
                    );
                },
                |ui| {
                    if !wt.is_main {
                        let btn = icon_button(ui, "×", theme.text_muted, theme)
                            .on_hover_text("delete worktree and branch");
                        delete_rect = Some(btn.rect);
                        if btn.clicked() {
                            delete_clicked = true;
                        }
                    }
                },
            );
        })
        .response
        .interact(egui::Sense::click());

    // Frame allocates its space at end-of-show, so its retroactive `interact`
    // registers *after* the inner button in egui's z-order — meaning clicks on
    // the × land on this row response, not the button.  Recover by routing
    // clicks whose position falls inside the button rect to delete.
    if resp.clicked() && !delete_clicked {
        if let (Some(rect), Some(pos)) = (delete_rect, resp.interact_pointer_pos()) {
            if rect.contains(pos) {
                delete_clicked = true;
            }
        }
    }

    let bg = if is_active {
        theme.row_active_bg
    } else if resp.hovered() {
        theme.row_hover_bg
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        let rect = egui::Rect::from_x_y_ranges(panel_x, resp.rect.y_range());
        ui.painter().set(bg_idx, egui::Shape::rect_filled(rect, 0.0, bg));
    }
    WorktreeAction { activate: resp.clicked() && !delete_clicked, delete: delete_clicked }
}

impl AlacritreeApp {
    fn reap_exited_sessions(&mut self) {
        let exited_ids: Vec<SessionId> =
            self.sessions.iter().filter(|s| s.is_exited()).map(|s| s.id).collect();
        for id in exited_ids {
            self.close_session(id);
        }
    }

    /// Handle workspace-switch requests from clicked notifications.  Only
    /// the most recent click is honored — if multiple toasts piled up, the
    /// user most likely meant the latest one.
    fn process_notification_actions(&mut self, ctx: &Context) {
        let mut latest: Option<WorkspaceKey> = None;
        while let Ok(ws) = self.notify_rx.try_recv() {
            latest = Some(ws);
        }
        let Some(ws) = latest else { return };
        match ws {
            None => self.activate_home(ctx),
            Some(p) => self.activate_worktree(ctx, &p),
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// Drain every session's PTY events and surface "needs attention" for
    /// any session the user isn't currently looking at.
    fn process_session_events(&mut self, ctx: &Context) {
        let visible_idx = self.active_session_index();
        // `viewport().focused` is `None` on platforms that don't report focus;
        // treat unknown as "focused" so we don't pile up stale attention dots.
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);

        for idx in 0..self.sessions.len() {
            let outcome = self.sessions[idx].drain_events();
            if !outcome.attention {
                continue;
            }
            let is_visible_to_user = Some(idx) == visible_idx && focused;
            if is_visible_to_user {
                continue;
            }
            // Only toast on the *transition* into needs_attention — otherwise
            // BEL + title-transition firing in the same idle cycle would
            // produce two toasts for the same "Claude is done" event.
            let was_attending = self.sessions[idx].needs_attention;
            self.sessions[idx].needs_attention = true;
            if !was_attending && self.config.ui.notifications {
                notify_attention(&self.sessions[idx], ctx);
            }
        }

        // Visible session shouldn't keep an attention marker once the user is
        // actually looking at it — covers tab switches, workspace switches,
        // and refocusing the window after stepping away.
        if focused {
            if let Some(idx) = visible_idx {
                self.sessions[idx].needs_attention = false;
            }
        }
    }

    fn workspace_needs_attention(&self, ws: &WorkspaceKey) -> bool {
        self.sessions.iter().any(|s| s.working_directory == *ws && s.needs_attention)
    }

    fn project_needs_attention(&self, project: &Project) -> bool {
        project.worktrees.iter().any(|wt| self.workspace_needs_attention(&Some(wt.path.clone())))
    }

    /// Prefer the active session's glyph so two parallel agents don't fight
    /// over which icon the sidebar shows.
    fn workspace_agent_glyph(&self, ws: &WorkspaceKey) -> Option<char> {
        let active_id = self.active_session.get(ws).copied();
        let mut active_glyph = None;
        let mut other_glyph = None;
        for s in &self.sessions {
            if s.working_directory != *ws {
                continue;
            }
            let Some(g) = s.agent_glyph() else { continue };
            if Some(s.id) == active_id {
                active_glyph = Some(g);
                break;
            }
            if other_glyph.is_none() {
                other_glyph = Some(g);
            }
        }
        active_glyph.or(other_glyph)
    }

    fn show_delete_dialog(&mut self, ctx: &Context) {
        let theme = self.theme;
        let danger = rgb_to_color32(self.config.palette.normal[1]);
        let Some(req) = self.pending_delete.as_ref() else {
            return;
        };
        let title = format!("Delete worktree `{}`?", req.worktree_name);
        let detail = match &req.branch {
            Some(b) => format!("Removes the worktree directory and deletes branch `{b}`."),
            None => "Removes the worktree directory.".to_string(),
        };
        let warning = dirty_warning(&req.dirty);

        let (cancel_via_key, confirm_via_key) = consume_modal_keys(ctx);

        let frame = modal_frame(&theme);
        let mut confirmed = false;
        let mut cancelled = false;

        let s = theme.ui_scale;
        let modal = egui::Modal::new(egui::Id::new("alacritree_delete_dialog")).frame(frame).show(
            ctx,
            |ui| {
                ui.set_width(360.0 * s);
                ui.spacing_mut().item_spacing.y = 6.0 * s;
                ui.label(RichText::new(title).color(theme.text).strong());
                ui.label(RichText::new(detail).color(theme.text_muted).small());
                if let Some(w) = &warning {
                    ui.label(RichText::new(w).color(danger).small());
                }
                ui.add_space(4.0 * s);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Enter to delete · Esc to cancel")
                            .color(theme.text_muted)
                            .small(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let delete = ui.add(
                            egui::Button::new(RichText::new("Delete").color(danger)).frame(false),
                        );
                        if delete.clicked() {
                            confirmed = true;
                        }
                        let cancel = ui.add(
                            egui::Button::new(RichText::new("Cancel").color(theme.text_dim))
                                .frame(false),
                        );
                        if cancel.clicked() {
                            cancelled = true;
                        }
                        focus_default(ui.ctx(), delete.id);
                    });
                });
            },
        );

        if confirm_via_key || confirmed {
            self.run_pending_delete();
            return;
        }
        if cancel_via_key || cancelled || modal.should_close() {
            self.pending_delete = None;
        }
    }

    fn run_pending_delete(&mut self) {
        let Some(req) = self.pending_delete.take() else {
            return;
        };
        let project_root = self.projects[req.project_idx].root.clone();

        // Drop sessions whose cwd is the worktree before deleting it; the PTY
        // would otherwise block the directory removal on some filesystems.
        self.sessions.retain(|s| s.working_directory.as_deref() != Some(&req.worktree_path));
        if self.current_workspace.as_deref() == Some(&req.worktree_path) {
            self.current_workspace = None;
        }
        self.active_session.remove(&Some(req.worktree_path.clone()));

        let force = req.dirty.is_dirty();
        if let Err(e) =
            wt::delete_worktree(&project_root, &req.worktree_path, req.branch.as_deref(), force)
        {
            self.last_error = Some(format!("delete failed: {e}"));
        }
        self.projects[req.project_idx].refresh();
    }

    fn show_create_dialog(&mut self, ctx: &Context) {
        let Some(state) = self.pending_create.take() else {
            return;
        };
        let next = match state {
            CreateState::Prompt { project_idx, branch, error } => {
                self.show_create_prompt(ctx, project_idx, branch, error)
            },
            CreateState::Running { project_idx, branch, mut steps, rx } => {
                let mut done: Option<Result<PathBuf, String>> = None;
                while let Ok(p) = rx.try_recv() {
                    match p {
                        Progress::Step(s) => steps.push(s),
                        Progress::Done(r) => done = Some(r),
                    }
                }
                self.show_create_running(ctx, project_idx, &branch, &steps);
                match done {
                    Some(result) => Some(CreateState::Done { project_idx, steps, result }),
                    None => Some(CreateState::Running { project_idx, branch, steps, rx }),
                }
            },
            CreateState::Done { project_idx, steps, result } => {
                if self.show_create_done(ctx, project_idx, &steps, &result) {
                    if let Ok(path) = &result {
                        self.projects[project_idx].refresh();
                        let path = path.clone();
                        self.activate_worktree(ctx, &path);
                    }
                    None
                } else {
                    Some(CreateState::Done { project_idx, steps, result })
                }
            },
        };
        self.pending_create = next;
    }

    fn show_create_prompt(
        &mut self,
        ctx: &Context,
        project_idx: usize,
        mut branch: String,
        mut error: Option<String>,
    ) -> Option<CreateState> {
        let theme = self.theme;
        let danger = rgb_to_color32(self.config.palette.normal[1]);
        let project_name = self.projects[project_idx].name.clone();
        let default_branch = self.projects[project_idx].default_branch.clone();
        let project_root = self.projects[project_idx].root.clone();

        let (cancel_via_key, confirm_via_key) = consume_modal_keys(ctx);
        let frame = modal_frame(&theme);
        let mut create_clicked = false;
        let mut cancelled = false;

        let s = theme.ui_scale;
        let modal = egui::Modal::new(egui::Id::new("alacritree_create_dialog")).frame(frame).show(
            ctx,
            |ui| {
                ui.set_width(380.0 * s);
                ui.spacing_mut().item_spacing.y = 6.0 * s;
                ui.label(
                    RichText::new(format!("New worktree in `{project_name}`"))
                        .color(theme.text)
                        .strong(),
                );
                ui.label(
                    RichText::new(match default_branch.as_deref() {
                        Some(b) => format!("Branched from origin/{b}"),
                        None => "No default branch detected — create may fail.".to_string(),
                    })
                    .color(theme.text_muted)
                    .small(),
                );
                let input_id = egui::Id::new("alacritree_create_input");
                let edit = egui::TextEdit::singleline(&mut branch)
                    .id(input_id)
                    .hint_text("branch name")
                    .desired_width(f32::INFINITY);
                let resp = ui.add(edit);
                focus_default(ui.ctx(), input_id);
                if resp.lost_focus() && resp.ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    create_clicked = true;
                }
                if let Some(e) = &error {
                    ui.label(RichText::new(e).color(danger).small());
                }
                ui.add_space(4.0 * s);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Enter to create · Esc to cancel")
                            .color(theme.text_muted)
                            .small(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(RichText::new("Create").color(theme.accent))
                                    .frame(false),
                            )
                            .clicked()
                        {
                            create_clicked = true;
                        }
                        if ui
                            .add(
                                egui::Button::new(RichText::new("Cancel").color(theme.text_dim))
                                    .frame(false),
                            )
                            .clicked()
                        {
                            cancelled = true;
                        }
                    });
                });
            },
        );

        if cancel_via_key || cancelled || modal.should_close() {
            return None;
        }
        if confirm_via_key || create_clicked {
            // Whitespace runs become single hyphens — `some text like this` → `some-text-like-this`.
            let canonical: String = branch.split_whitespace().collect::<Vec<_>>().join("-");
            if let Err(msg) = wt::validate_branch_name(&canonical) {
                error = Some(msg);
                return Some(CreateState::Prompt { project_idx, branch, error });
            }
            let req = CreateRequest { project_root, default_branch, branch: canonical.clone() };
            let rx = wt::spawn_create(req, ctx.clone());
            return Some(CreateState::Running {
                project_idx,
                branch: canonical,
                steps: Vec::new(),
                rx,
            });
        }
        Some(CreateState::Prompt { project_idx, branch, error })
    }

    fn show_create_running(
        &self,
        ctx: &Context,
        project_idx: usize,
        branch: &str,
        steps: &[String],
    ) {
        let theme = self.theme;
        let project_name = self.projects[project_idx].name.clone();
        let frame = modal_frame(&theme);
        let s = theme.ui_scale;
        let _ = egui::Modal::new(egui::Id::new("alacritree_create_dialog")).frame(frame).show(
            ctx,
            |ui| {
                ui.set_width(380.0 * s);
                ui.spacing_mut().item_spacing.y = 6.0 * s;
                ui.label(
                    RichText::new(format!("Creating `{branch}` in `{project_name}`"))
                        .color(theme.text)
                        .strong(),
                );
                ui.add_space(4.0 * s);
                for (i, step) in steps.iter().enumerate() {
                    let is_last = i + 1 == steps.len();
                    let bullet_color = if is_last { theme.accent } else { theme.text_dim };
                    let text_color = if is_last { theme.text } else { theme.text_dim };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("•").color(bullet_color));
                        ui.label(RichText::new(step).color(text_color).small());
                    });
                }
                if steps.is_empty() {
                    ui.label(RichText::new("Starting…").color(theme.text_muted).small());
                }
            },
        );
    }

    fn show_create_done(
        &self,
        ctx: &Context,
        project_idx: usize,
        steps: &[String],
        result: &Result<PathBuf, String>,
    ) -> bool {
        let theme = self.theme;
        let danger = rgb_to_color32(self.config.palette.normal[1]);
        let ok = rgb_to_color32(self.config.palette.normal[2]);
        let project_name = self.projects[project_idx].name.clone();
        let frame = modal_frame(&theme);
        let mut close = false;
        let (cancel_via_key, confirm_via_key) = consume_modal_keys(ctx);

        let s = theme.ui_scale;
        let modal = egui::Modal::new(egui::Id::new("alacritree_create_dialog")).frame(frame).show(
            ctx,
            |ui| {
                ui.set_width(380.0 * s);
                ui.spacing_mut().item_spacing.y = 6.0 * s;
                let (title, color) = match result {
                    Ok(_) => (format!("Created worktree in `{project_name}`"), ok),
                    Err(_) => ("Worktree creation failed".to_string(), danger),
                };
                ui.label(RichText::new(title).color(color).strong());
                let last = steps.len().saturating_sub(1);
                for (i, step) in steps.iter().enumerate() {
                    let failed_step = result.is_err() && i == last;
                    let bullet_color = if failed_step { danger } else { ok };
                    let text_color = if failed_step { danger } else { theme.text_dim };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("•").color(bullet_color));
                        ui.label(RichText::new(step).color(text_color).small());
                    });
                }
                if let Err(e) = result {
                    ui.add_space(4.0 * s);
                    ui.label(RichText::new(e).color(danger).small());
                }
                ui.add_space(4.0 * s);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if result.is_ok() { "Open" } else { "Close" };
                    let btn = ui.add(
                        egui::Button::new(RichText::new(label).color(theme.accent)).frame(false),
                    );
                    if btn.clicked() {
                        close = true;
                    }
                    focus_default(ui.ctx(), btn.id);
                });
            },
        );

        if confirm_via_key || cancel_via_key || close || modal.should_close() {
            return true;
        }
        false
    }

    fn show_quit_dialog(&mut self, ctx: &Context) {
        let theme = self.theme;
        let danger = rgb_to_color32(self.config.palette.normal[1]);
        let n = self.sessions.len();

        let (cancel_via_key, confirm_via_key) = consume_modal_keys(ctx);
        let frame = modal_frame(&theme);
        let mut quit_clicked = false;
        let mut cancel_clicked = false;

        let s = theme.ui_scale;
        let modal = egui::Modal::new(egui::Id::new("alacritree_quit_dialog")).frame(frame).show(
            ctx,
            |ui| {
                ui.set_width(320.0 * s);
                ui.spacing_mut().item_spacing.y = 6.0 * s;
                ui.label(RichText::new("Quit alacritree?").color(theme.text).strong());
                let msg = match n {
                    0 => "No sessions running.".to_string(),
                    1 => "1 session will be terminated.".to_string(),
                    n => format!("{n} sessions will be terminated."),
                };
                ui.label(RichText::new(msg).color(theme.text_muted).small());
                ui.add_space(4.0 * s);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Enter to quit · Esc to cancel")
                            .color(theme.text_muted)
                            .small(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let quit_id = egui::Id::new("alacritree_quit_btn");
                        let quit = ui.add(
                            egui::Button::new(RichText::new("Quit").color(danger)).frame(false),
                        );
                        if quit.clicked() {
                            quit_clicked = true;
                        }
                        if ui
                            .add(
                                egui::Button::new(RichText::new("Cancel").color(theme.text_dim))
                                    .frame(false),
                            )
                            .clicked()
                        {
                            cancel_clicked = true;
                        }
                        focus_default(ui.ctx(), quit_id);
                    });
                });
            },
        );

        if confirm_via_key || quit_clicked {
            self.quit_dialog_open = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if cancel_via_key || cancel_clicked || modal.should_close() {
            self.quit_dialog_open = false;
        }
    }
}

impl eframe::App for AlacritreeApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let bg = self.theme.terminal_bg;
        let n = |c: u8| c as f32 / 255.0;
        [n(bg.r()), n(bg.g()), n(bg.b()), self.config.window.opacity]
    }

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let modal_open = self.is_modal_open();
        if !modal_open {
            self.handle_shortcuts(ctx);
        }
        self.process_notification_actions(ctx);
        self.process_session_events(ctx);
        let theme = self.theme;
        // GL clear is the sole source of the bg when opacity < 1; painting any
        // panel fill on top would compound the alpha through egui's blend.
        let translucent = self.config.window.opacity < 1.0;
        let sidebar_fill = if translucent { Color32::TRANSPARENT } else { theme.sidebar_bg };
        let central_fill = if translucent { Color32::TRANSPARENT } else { theme.terminal_bg };

        let panel_frame = Frame::default().fill(sidebar_fill).inner_margin(Margin::same(8));

        if self.show_left_sidebar {
            let r = self.show_project_sidebar(ctx, panel_frame.clone());
            paint_panel_border(ctx, r.right(), r.y_range(), theme.sidebar_border);
        }

        if self.show_right_sidebar {
            let r = self.show_git_sidebar(ctx, panel_frame);
            paint_panel_border(ctx, r.left(), r.y_range(), theme.sidebar_border);
        }

        egui::CentralPanel::default()
            .frame(Frame::default().fill(central_fill).inner_margin(Margin::same(0)))
            .show(ctx, |ui| {
                self.show_tab_strip(ui);

                if let Some(err) = self.last_error.as_deref() {
                    ui.label(
                        RichText::new(err)
                            .color(rgb_to_color32(self.config.palette.normal[1]))
                            .monospace(),
                    );
                    return;
                }

                if self.active_session_index().is_none() {
                    self.ensure_active_session(ctx);
                }

                let Some(idx) = self.active_session_index() else {
                    ui.label(
                        RichText::new("no session — Ctrl+T to open one").color(theme.text_dim),
                    );
                    return;
                };
                let session = &mut self.sessions[idx];
                let _ = terminal_view::show(
                    ui,
                    session,
                    &self.config,
                    !modal_open,
                    &mut self.builtin_glyphs,
                );
            });

        if self.pending_create.is_some() {
            self.show_create_dialog(ctx);
        }
        if self.pending_delete.is_some() {
            self.show_delete_dialog(ctx);
        }
        if self.quit_dialog_open {
            self.show_quit_dialog(ctx);
        }

        self.reap_exited_sessions();
    }
}

/// Spawn a throwaway thread so `notify-rust`'s synchronous D-Bus / WinRT
/// calls don't stall the egui paint loop.  On Linux the thread sticks around
/// for `wait_for_action` and posts the session's workspace back through
/// `NOTIFY_TX` when the user clicks the notification.
fn notify_attention(session: &Session, ctx: &egui::Context) {
    let where_label = session
        .working_directory
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| session.title.clone());
    let body = if where_label.is_empty() {
        "Session is waiting for input".to_string()
    } else {
        format!("{where_label} is waiting for input")
    };
    let key = session.working_directory.clone();
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("alacritree-notify".into())
        .spawn(move || notify_worker(body, key, ctx))
        .ok();
}

#[cfg(all(unix, not(target_os = "macos")))]
fn notify_worker(body: String, key: WorkspaceKey, ctx: egui::Context) {
    // `default` is the action id freedesktop notifiers fire on body-click.
    let result = notify_rust::Notification::new()
        .summary("alacritree")
        .body(&body)
        .action("default", "Open")
        .show();
    let handle = match result {
        Ok(h) => h,
        Err(e) => {
            log::debug!("desktop notification failed: {e}");
            return;
        },
    };
    handle.wait_for_action(|action| {
        if action == "__closed" {
            return;
        }
        if let Some(lock) = NOTIFY_TX.get() {
            if let Ok(tx) = lock.lock() {
                let _ = tx.send(key.clone());
                ctx.request_repaint();
            }
        }
    });
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn notify_worker(body: String, _key: WorkspaceKey, _ctx: egui::Context) {
    // mac-notification-sys / WinRT don't expose blocking action waits via
    // notify-rust today — fall back to a fire-and-forget toast.
    if let Err(e) = notify_rust::Notification::new().summary("alacritree").body(&body).show() {
        log::debug!("desktop notification failed: {e}");
    }
}
