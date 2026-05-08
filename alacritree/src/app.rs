use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::CreationContext;
use egui::{Color32, Context, Frame, Margin, RichText, ScrollArea, SidePanel, Stroke};

use crate::bindings::{BindingAction, NamedAction};
use crate::clipboard::{self, Target};
use crate::colors::rgb_to_color32;
use crate::config::Config;
use crate::git_status::{ChangeKind, FileChange, StatusCache};
use crate::projects::{Project, Worktree};
use crate::session::{Session, SessionId, TermSize};
use crate::state::{self, PersistedProject, PersistedState};
use crate::terminal_view;

/// Workspace identifier — the working directory of a worktree, or `None` for
/// the implicit "home" workspace where sessions inherit `$PWD`.
type WorkspaceKey = Option<PathBuf>;

/// UI colors resolved against the user's config.  The sidebar background
/// follows the terminal background by default; everything else is a small
/// perceptual offset from it so panels read as panels.
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
}

impl Theme {
    fn from_config(config: &Config) -> Self {
        let terminal_bg = rgb_to_color32(config.palette.bg);
        let sidebar_bg = config.ui.sidebar_background.unwrap_or(terminal_bg);
        let text = config
            .ui
            .sidebar_foreground
            .unwrap_or_else(|| rgb_to_color32(config.palette.fg));
        let accent = config
            .ui
            .sidebar_accent
            .unwrap_or_else(|| rgb_to_color32(config.palette.normal[4])); // ANSI blue
        let border = config
            .ui
            .sidebar_border
            .unwrap_or_else(|| lighten(sidebar_bg, 0.10));
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
        }
    }
}

/// Move `c` toward white by `amount` (0..1).  Used to derive panel-on-panel
/// shades from a single base color.
fn lighten(c: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let mix = |x: u8| -> u8 {
        let v = x as f32;
        (v + (255.0 - v) * amount).round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(mix(c.r()), mix(c.g()), mix(c.b()))
}

/// Blend `c` toward `target` by `amount` (0..1).
fn blend_toward(c: Color32, target: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| -> u8 {
        let av = a as f32;
        let bv = b as f32;
        (av + (bv - av) * amount).round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(
        mix(c.r(), target.r()),
        mix(c.g(), target.g()),
        mix(c.b(), target.b()),
    )
}

pub struct AlacritreeApp {
    show_left_sidebar: bool,
    show_right_sidebar: bool,
    sessions: Vec<Session>,
    /// Currently visible workspace.  `None` means the home workspace (no
    /// associated worktree).  Sessions are bucketed by their
    /// `working_directory` matching this value.
    current_workspace: WorkspaceKey,
    /// Per-workspace, the id of the last-active session.  Survives session
    /// closures because we look up by id, not by index.
    active_session: HashMap<WorkspaceKey, SessionId>,
    projects: Vec<Project>,
    git_status: HashMap<PathBuf, StatusCache>,
    config: Config,
    theme: Theme,
    last_error: Option<String>,
    quit_dialog_open: bool,
}

impl AlacritreeApp {
    pub fn new(cc: &CreationContext<'_>, config: Config) -> Self {
        let theme = Theme::from_config(&config);

        crate::fonts::install_terminal_font(&cc.egui_ctx, config.font.normal_family.as_deref());

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = theme.terminal_bg;
        visuals.window_fill = theme.terminal_bg;
        visuals.extreme_bg_color = theme.terminal_bg;
        cc.egui_ctx.set_visuals(visuals);

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

    /// Switch the visible workspace to `path` and ensure it has at least one
    /// session.  Existing sessions for `path` are kept alive — switching
    /// preserves running processes.
    fn activate_worktree(&mut self, ctx: &Context, path: &Path) {
        self.current_workspace = Some(path.to_path_buf());
        self.ensure_active_session(ctx);
    }

    /// Switch to the implicit home workspace.
    fn activate_home(&mut self, ctx: &Context) {
        self.current_workspace = None;
        self.ensure_active_session(ctx);
    }

    /// Make sure `current_workspace` has an active, live session.  Fall back
    /// to any other session in the workspace; spawn a fresh one if none exist.
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

        // If this session was the active one for its workspace, fall back to
        // any other session still running there.
        if self.active_session.get(&workspace).copied() == Some(id) {
            let fallback = self
                .sessions
                .iter()
                .find(|s| s.working_directory == workspace)
                .map(|s| s.id);
            match fallback {
                Some(new_id) => {
                    self.active_session.insert(workspace, new_id);
                }
                None => {
                    self.active_session.remove(&workspace);
                }
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

    /// Cycle through tabs in the current workspace.  `delta = +1` next,
    /// `delta = -1` previous.  No-op if 0 or 1 tabs.
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

    /// Cycle through workspaces (Home + each project's worktrees, in sidebar
    /// order).  `delta` is +1 / -1.  Spawns a new session for the destination
    /// workspace if none exists, like clicking it in the sidebar would.
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
            }
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

    fn handle_shortcuts(&mut self, ctx: &Context) {
        let mut sidebars_changed = false;
        let mut cycle_tabs_delta: Option<i32> = None;
        let mut cycle_ws_delta: Option<i32> = None;
        let mut quit_requested = false;
        ctx.input_mut(|i| {
            if i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::B,
            )) {
                self.show_left_sidebar = !self.show_left_sidebar;
                sidebars_changed = true;
            }
            if i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::G,
            )) {
                self.show_right_sidebar = !self.show_right_sidebar;
                sidebars_changed = true;
            }
            // Tab cycling within the current workspace.
            if i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::Tab,
            )) {
                cycle_tabs_delta = Some(1);
            }
            if i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                egui::Key::Tab,
            )) {
                cycle_tabs_delta = Some(-1);
            }
            // Workspace cycling: Ctrl+Alt+Right / Ctrl+Alt+Left.
            if i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL | egui::Modifiers::ALT,
                egui::Key::ArrowRight,
            )) {
                cycle_ws_delta = Some(1);
            }
            if i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL | egui::Modifiers::ALT,
                egui::Key::ArrowLeft,
            )) {
                cycle_ws_delta = Some(-1);
            }
            // Ctrl+Q: open the quit-confirmation dialog.
            if i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::Q,
            )) {
                quit_requested = true;
            }
        });

        // Ctrl+T spawns a new session.  Done outside `consume_shortcut` so the
        // PTY's input handler doesn't also see the keystroke.
        let ctrl_t = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::T,
            ))
        });
        if ctrl_t {
            // New tabs land in the currently visible workspace, so they share
            // the worktree's working directory.
            let ws = self.current_workspace.clone();
            if let Err(e) = self.spawn_session(ctx, ws) {
                self.last_error = Some(format!("failed to spawn shell: {e}"));
            }
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

        // User-defined keyboard.bindings from the alacritty config.  Run
        // *after* built-in shortcuts so we don't shadow them, but *before*
        // events flow through to the terminal so a binding takes precedence
        // over plain text input.
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
            }
            BindingAction::Named(NamedAction::Paste) => {
                if let Some(text) = clipboard::read(Target::Clipboard) {
                    if let Some(idx) = self.active_session_index() {
                        self.sessions[idx].write(text.into_bytes());
                    }
                }
            }
            BindingAction::Named(NamedAction::PasteSelection) => {
                if let Some(text) = clipboard::read(Target::Primary) {
                    if let Some(idx) = self.active_session_index() {
                        self.sessions[idx].write(text.into_bytes());
                    }
                }
            }
            BindingAction::Named(NamedAction::Copy) => {
                if let Some(idx) = self.active_session_index() {
                    if let Some(text) = self.sessions[idx].term.lock().selection_to_string() {
                        if !text.is_empty() {
                            clipboard::write(Target::Clipboard, &text);
                        }
                    }
                }
            }
            BindingAction::Named(NamedAction::SpawnNewInstance) => {
                let ws = self.current_workspace.clone();
                if let Err(e) = self.spawn_session(ctx, ws) {
                    self.last_error = Some(format!("failed to spawn shell: {e}"));
                }
            }
            BindingAction::Named(NamedAction::Quit) => {
                self.quit_dialog_open = true;
            }
            BindingAction::Named(other) => {
                // Scroll / font-size actions: best-effort implementation.
                self.dispatch_scroll_or_other(other);
            }
            BindingAction::Unsupported(name) => {
                log::debug!("unsupported keyboard binding action: {name}");
            }
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

    /// Thin tab indicator at the top of the terminal pane.
    ///
    /// Hidden when the current workspace has fewer than 2 tabs (so a single
    /// shell looks chrome-free).  With multiple tabs, draws an N-segment
    /// horizontal strip; the active segment is highlighted, others are dim.
    /// Clicking a segment activates that tab.
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
        let segment_width = ((avail - gap * (indices.len() as f32 - 1.0)) / indices.len() as f32).max(1.0);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(avail, strip_height + 2.0),
            egui::Sense::hover(),
        );

        let mut activate: Option<SessionId> = None;
        for (i, &session_idx) in indices.iter().enumerate() {
            let x0 = rect.min.x + i as f32 * (segment_width + gap);
            let seg_rect = egui::Rect::from_min_size(
                egui::pos2(x0, rect.min.y + 1.0),
                egui::vec2(segment_width, strip_height),
            );
            let is_active = active_idx == Some(session_idx);
            // Expand the click target a few pixels above so it's easier to hit.
            let click_rect = seg_rect.expand2(egui::vec2(0.0, 4.0));
            let id = ui.id().with(("tab_strip", self.sessions[session_idx].id));
            let resp = ui.interact(click_rect, id, egui::Sense::click());
            let color = if is_active {
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
            // Show the tab title when hovered for a hint without taking space.
            if resp.hovered() {
                resp.on_hover_text(&self.sessions[session_idx].title);
            }
        }

        if let Some(id) = activate {
            self.set_active_in_current_workspace(id);
        }
    }

    fn show_project_sidebar(&mut self, ctx: &Context, panel_frame: Frame) {
        let activate_request: std::cell::Cell<Option<PathBuf>> = std::cell::Cell::new(None);
        let mut add_project_clicked = false;
        let mut refresh_idx: Option<usize> = None;
        let mut remove_idx: Option<usize> = None;
        let mut expand_toggled = false;
        let mut home_clicked = false;
        let theme = self.theme;

        SidePanel::left("left_sidebar")
            .resizable(true)
            .default_width(240.0)
            .min_width(180.0)
            .frame(panel_frame)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Projects").color(theme.text).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("+").color(theme.text_dim)).frame(false))
                            .on_hover_text("add project")
                            .clicked()
                        {
                            add_project_clicked = true;
                        }
                    });
                });
                ui.separator();

                ScrollArea::vertical().show(ui, |ui| {
                    // Implicit "home" workspace — sessions with no working
                    // directory live here.  Always visible so the user can
                    // come back to it after entering a worktree.
                    if home_row(ui, self.current_workspace.is_none(), &theme).clicked() {
                        home_clicked = true;
                    }
                    ui.add_space(2.0);

                    if self.projects.is_empty() {
                        ui.label(RichText::new("Click + to add a project.").color(theme.text_dim).small());
                        ui.add_space(4.0);
                        ui.label(RichText::new("Ctrl+B to toggle").small().color(theme.text_muted));
                    }

                    for (idx, project) in self.projects.iter_mut().enumerate() {
                        let header_resp = ui.horizontal(|ui| {
                            let arrow = if project.expanded { "▾" } else { "▸" };
                            let toggle = ui
                                .add(egui::Button::new(RichText::new(arrow).color(theme.text_dim)).frame(false));
                            if toggle.clicked() {
                                project.expanded = !project.expanded;
                                expand_toggled = true;
                            }
                            ui.label(RichText::new(&project.name).color(theme.text).strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(egui::Button::new(RichText::new("×").color(theme.text_muted)).frame(false))
                                        .on_hover_text("remove from sidebar")
                                        .clicked()
                                    {
                                        remove_idx = Some(idx);
                                    }
                                    if ui
                                        .add(egui::Button::new(RichText::new("⟲").color(theme.text_muted)).frame(false))
                                        .on_hover_text("refresh worktrees")
                                        .clicked()
                                    {
                                        refresh_idx = Some(idx);
                                    }
                                },
                            );
                        });
                        let _ = header_resp;

                        if project.expanded {
                            for wt in &project.worktrees {
                                let is_active =
                                    self.current_workspace.as_deref() == Some(&wt.path);
                                if worktree_row(ui, wt, is_active, &theme).clicked() {
                                    activate_request.set(Some(wt.path.clone()));
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
    }

    fn active_session_path(&self) -> Option<PathBuf> {
        // The right sidebar tracks the visible workspace, not necessarily the
        // active session — they're equivalent except in the "home" workspace
        // where there's no path to show git status for.
        self.current_workspace.clone()
    }

    fn project_default_branch_for(&self, path: &Path) -> Option<String> {
        // If the active session's path is inside or equal to a known project's
        // worktree, use that project's default branch as a hint for the diff.
        for project in &self.projects {
            for wt in &project.worktrees {
                if wt.path == path {
                    return project.default_branch.clone();
                }
            }
        }
        None
    }

    fn show_git_sidebar(&mut self, ctx: &Context, panel_frame: Frame) {
        let theme = self.theme;
        let palette = self.config.palette.clone();
        SidePanel::right("right_sidebar")
            .resizable(true)
            .default_width(300.0)
            .min_width(220.0)
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
                            ui.label(RichText::new("Ctrl+G to toggle").small().color(theme.text_muted));
                        });
                        return;
                    }
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
                        ui.label(RichText::new(err).color(rgb_to_color32(palette.normal[1])).small());
                        return;
                    }

                    ui.label(
                        RichText::new(path.display().to_string())
                            .color(theme.text_muted)
                            .small(),
                    );
                    if let Some(branch) = &status.branch {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("on").color(theme.text_muted).small());
                            ui.label(RichText::new(branch).color(theme.accent));
                            if let Some(default) = &status.default_branch {
                                if Some(branch) != Some(default) {
                                    ui.label(RichText::new("vs").color(theme.text_muted).small());
                                    ui.label(RichText::new(default).color(theme.text_dim).small());
                                }
                            }
                        });
                    }
                    ui.add_space(6.0);

                    section(
                        ui,
                        &theme,
                        "Staged",
                        status.staged.len(),
                        |ui| {
                            for f in &status.staged {
                                file_row(ui, f, &theme, &palette);
                            }
                        },
                    );

                    section(
                        ui,
                        &theme,
                        "Unstaged",
                        status.unstaged.len(),
                        |ui| {
                            for f in &status.unstaged {
                                file_row(ui, f, &theme, &palette);
                            }
                        },
                    );

                    if !status.branch_diff.is_empty() {
                        let label = match (&status.default_branch, &status.default_branch_resolved) {
                            (Some(b), _) => format!("Changes vs {}", b),
                            _ => "Changes vs default".to_string(),
                        };
                        let added = rgb_to_color32(palette.normal[2]);
                        let removed = rgb_to_color32(palette.normal[1]);
                        section(ui, &theme, &label, status.branch_diff.len(), |ui| {
                            for stat in &status.branch_diff {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&stat.path).color(theme.text_dim).monospace());
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
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
                                });
                            }
                        });
                    }
                });
            });
    }
}

fn section<R>(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    count: usize,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).color(theme.text).strong());
        ui.label(RichText::new(format!("{count}")).color(theme.text_muted).small());
    });
    if count == 0 {
        ui.label(RichText::new("·").color(theme.text_muted).small());
    } else {
        add_contents(ui);
    }
    ui.add_space(8.0);
}

fn file_row(ui: &mut egui::Ui, change: &FileChange, theme: &Theme, palette: &crate::config::Palette) {
    ui.horizontal(|ui| {
        let color = match change.kind {
            ChangeKind::Added | ChangeKind::Untracked => rgb_to_color32(palette.normal[2]),
            ChangeKind::Modified => rgb_to_color32(palette.normal[3]),
            ChangeKind::Deleted => rgb_to_color32(palette.normal[1]),
            ChangeKind::Renamed => rgb_to_color32(palette.normal[4]),
            ChangeKind::Conflicted => rgb_to_color32(palette.bright[1]),
        };
        ui.label(RichText::new(change.kind.glyph()).color(color).monospace().small());
        ui.label(RichText::new(&change.path).color(theme.text_dim).monospace().small());
    });
}

fn home_row(ui: &mut egui::Ui, is_active: bool, theme: &Theme) -> egui::Response {
    let bg_idx = ui.painter().add(egui::Shape::Noop);

    let frame = Frame::default().inner_margin(Margin { left: 6, right: 6, top: 3, bottom: 3 });
    let resp = frame
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("⌂")
                        .color(if is_active { theme.accent } else { theme.text_muted })
                        .small(),
                );
                ui.label(
                    RichText::new("Home")
                        .color(if is_active { theme.text } else { theme.text_dim })
                        .strong(),
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
        ui.painter().set(bg_idx, egui::Shape::rect_filled(resp.rect, 0.0, bg));
    }
    resp
}

fn worktree_row(
    ui: &mut egui::Ui,
    wt: &Worktree,
    is_active: bool,
    theme: &Theme,
) -> egui::Response {
    // Reserve a paint slot *before* the row's content so the hover/active
    // background can be filled in beneath the labels once we know the state.
    // (Painting it after would put it on top of the text — the bug we're fixing.)
    let bg_idx = ui.painter().add(egui::Shape::Noop);

    let frame = Frame::default()
        .inner_margin(Margin { left: 16, right: 6, top: 3, bottom: 3 });
    let resp = frame
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let icon = if wt.is_main { "●" } else { "○" };
                ui.label(
                    RichText::new(icon)
                        .color(if is_active { theme.accent } else { theme.text_muted })
                        .small(),
                );
                ui.label(
                    RichText::new(&wt.name)
                        .color(if is_active { theme.text } else { theme.text_dim }),
                );
                if let Some(branch) = &wt.branch {
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.label(RichText::new(branch).small().color(theme.text_muted));
                        },
                    );
                }
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
        ui.painter()
            .set(bg_idx, egui::Shape::rect_filled(resp.rect, 0.0, bg));
    }
    resp
}

impl AlacritreeApp {
    /// Drop sessions whose child shell has exited.  We do this once per frame
    /// after rendering so the just-emitted ChildExit event has been drained.
    fn reap_exited_sessions(&mut self) {
        let exited_ids: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|s| s.is_exited())
            .map(|s| s.id)
            .collect();
        for id in exited_ids {
            self.close_session(id);
        }
    }

    fn show_quit_dialog(&mut self, ctx: &Context) {
        let theme = self.theme;
        let modal = egui::Modal::new(egui::Id::new("alacritree_quit_dialog")).show(ctx, |ui| {
            ui.set_min_width(280.0);
            ui.heading(RichText::new("Quit alacritree?").color(theme.text));
            ui.add_space(6.0);
            let n = self.sessions.len();
            let msg = if n == 0 {
                "No sessions are running.".to_string()
            } else if n == 1 {
                "1 session will be terminated.".to_string()
            } else {
                format!("{n} sessions will be terminated.")
            };
            ui.label(RichText::new(msg).color(theme.text_dim));
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RichText::new("Quit").color(Color32::from_rgb(0xff, 0x6b, 0x6b))).clicked() {
                    self.quit_dialog_open = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Cancel").clicked() {
                    self.quit_dialog_open = false;
                }
            });
        });
        // Clicking outside the modal also dismisses.
        if modal.should_close() {
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
        self.handle_shortcuts(ctx);

        let theme = self.theme;
        // GL clear is the sole source of the bg when opacity < 1; painting any
        // panel fill on top would compound the alpha through egui's blend.
        let translucent = self.config.window.opacity < 1.0;
        let sidebar_fill = if translucent { Color32::TRANSPARENT } else { theme.sidebar_bg };
        let central_fill = if translucent { Color32::TRANSPARENT } else { theme.terminal_bg };

        let panel_frame = Frame::default()
            .fill(sidebar_fill)
            .stroke(Stroke::new(1.0, theme.sidebar_border))
            .inner_margin(Margin::same(8));

        if self.show_left_sidebar {
            self.show_project_sidebar(ctx, panel_frame.clone());
        }

        if self.show_right_sidebar {
            self.show_git_sidebar(ctx, panel_frame);
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

                // If the visible workspace has no live session yet, spawn one
                // lazily — happens on first launch and after closing the last
                // tab in a workspace.
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
                let _ = terminal_view::show(ui, session, &self.config);
            });

        if self.quit_dialog_open {
            self.show_quit_dialog(ctx);
        }

        self.reap_exited_sessions();
    }
}
