use std::time::Instant;

use glutin::surface::Rect;

use crate::config::ui_config::{Scrollbar as ScrollbarConfig, ScrollbarMode};

use super::SizeInfo;

/// Keeps track of when the scrollbar should be visible or fading.
#[derive(Debug)]
pub struct Scrollbar {
    config: ScrollbarConfig,
    /// Display offset, that was last used to draw the scrollbar.
    display_offset: usize,
    /// Total lines, that was last used to draw the scrollbar.
    total_lines: usize,
    last_change: Option<Instant>,
}

impl From<&ScrollbarConfig> for Scrollbar {
    fn from(value: &ScrollbarConfig) -> Self {
        Scrollbar { config: value.clone(), display_offset: 0, total_lines: 0, last_change: None }
    }
}

impl Scrollbar {
    pub fn update_config(&mut self, config: &ScrollbarConfig) {
        self.config = config.clone();
    }

    /// Returns whether the scrollbar position or height needs an update.
    pub fn update(&mut self, display_offset: usize, total_lines: usize) -> bool {
        if self.display_offset != display_offset {
            self.display_offset = display_offset;
            self.total_lines = total_lines;
            self.last_change = Some(Instant::now());
            true
        } else if self.total_lines != total_lines {
            self.total_lines = total_lines;
            self.last_change = Some(Instant::now());
            true
        } else {
            false
        }
    }

    fn last_change_time(&self) -> Option<Instant> {
        self.last_change
    }

    fn clear_change_time(&mut self) {
        self.last_change = None;
    }

    pub fn intensity(&mut self) -> Option<f32> {
        let opacity = match self.config.mode {
            ScrollbarMode::Never => {
                return None;
            },
            ScrollbarMode::Fading => {
                let last_scroll = self.last_change_time()?;
                let timeout = (Instant::now() - last_scroll).as_secs_f32();
                if timeout <= self.config.fade_wait_in_secs {
                    self.config.opacity.as_f32()
                } else {
                    let current_fade_time = timeout - self.config.fade_wait_in_secs;
                    if current_fade_time < self.config.fade_time_in_secs {
                        // Fading progress from 0.0 to 1.0.
                        let fading_progress = current_fade_time / self.config.fade_time_in_secs;
                        (1.0 - fading_progress) * self.config.opacity.as_f32()
                    } else {
                        self.clear_change_time();
                        return None;
                    }
                }
            },
            ScrollbarMode::Always => self.config.opacity.as_f32(),
        };
        Some(opacity)
    }

    pub fn bg_rect(&self, display_size: SizeInfo, scale_factor: f32) -> Rect {
        let scrollbar_margin_y = self.config.margin.y * scale_factor;
        let scrollbar_margin_x = self.config.margin.x * scale_factor;

        let background_area_height: f32 = display_size.height - 2.0 * scrollbar_margin_y;

        let scrollbar_width = self.config.width * scale_factor;
        let x = display_size.width - scrollbar_width - scrollbar_margin_x;
        Rect {
            x: x.floor() as i32,
            y: scrollbar_margin_y.floor() as i32,
            width: scrollbar_width.ceil() as i32,
            height: background_area_height.ceil() as i32,
        }
    }

    pub fn rect_from_bg_rect(
        &self,
        bg_rect: Rect,
        display_size: SizeInfo,
        scale_factor: f32,
    ) -> Rect {
        let height_fraction = display_size.screen_lines as f32 / self.total_lines as f32;
        let scrollbar_height =
            (height_fraction * bg_rect.height as f32).max(self.config.min_height * scale_factor);

        let y_progress = self.display_offset as f32 / self.total_lines as f32;
        let y = y_progress * bg_rect.height as f32 + bg_rect.y as f32;

        Rect {
            x: bg_rect.x,
            y: y.floor() as i32,
            width: bg_rect.width,
            height: scrollbar_height.ceil() as i32,
        }
    }
}
