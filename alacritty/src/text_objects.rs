use std::collections::{HashMap, HashSet};

use font::Metrics;
use glutin::event::{ElementState, ModifiersState};
use regex::bytes::RegexSet;

use crate::config::text_objects::TextObject;
use crate::event::Mouse;
use crate::renderer::rects::{RenderLine, RenderRect};

use alacritty_terminal::grid::Indexed;
use alacritty_terminal::index::Point;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{color, SizeInfo};

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayTxtObj {
    start: Point,
    end: Point,
    priority: usize,
    pub action: Vec<String>,
}

impl DisplayTxtObj {
    pub fn rects(&self, metrics: &Metrics, size: &SizeInfo) -> Vec<RenderRect> {
        RenderLine { start: self.start, end: self.end, color: color::RED }.rects(
            Flags::UNDERLINE,
            metrics,
            size,
        )
    }
}

pub struct DisplayTextObjects {
    // regex_set is used to check text objects match in a single scan of display
    regex_set: RegexSet,
    display_objects: Vec<DisplayTxtObj>,
}

impl DisplayTextObjects {
    pub fn new(txob: &HashMap<String, TextObject>) -> DisplayTextObjects {
        DisplayTextObjects {
            regex_set: RegexSet::new(
                // Build the set only on text-objects that were correctly configured
                txob.values().filter_map(|cfg| cfg.search.as_ref().map(|s| s.as_str())),
            )
            .unwrap(), // already validated when deserialized TextObject
            display_objects: Vec::new(),
        }
    }

    pub fn find_objects(
        &mut self,
        config_txt_objects: &HashMap<String, TextObject>,
        display_text: &[u8],
        display_cells: &[Indexed<Cell>],
    ) {
        // Do a light-weight one-pass scan for all text-objects
        let matches: HashSet<_> = self.regex_set.matches(display_text).into_iter().collect();

        // If there was any match then we'll extract more information for triggering actions
        self.display_objects = config_txt_objects.values()
            // Check that search regex was correctly parsed and text-object is enabled
            .filter(|cfg| cfg.search.is_some())
            .enumerate()
            // Require the regex matched something on display and we have action arguments
            .filter(|(idx, cfg)| matches.contains(idx) && cfg.action.len() >= 2)
            .flat_map(|(_, cfg)| cfg.search.as_ref().unwrap()
                      .find_iter(&display_text)
                      .map(move |amatch| {
                          // Find limits of match
                          let start = &display_cells[amatch.start()];
                          let end = &display_cells[amatch.end()-1];
                          // Build action from template by re-writing last argument
                          let mut action = cfg.action[0..cfg.action.len()-1].to_owned();
                          let tpl_arg = cfg.search.as_ref().unwrap()
                              .replace(&display_text[amatch.start()..amatch.end()],
                                       cfg.action.last().unwrap().as_bytes());
                          action.push(String::from_utf8(tpl_arg.to_vec()).unwrap());
                          DisplayTxtObj {
                              start: Point::new(start.line,start.column),
                              end: Point::new(end.line, end.column),
                              priority: cfg.priority,
                              action,
                          }

                      }))
            .collect()
    }

    pub fn highlighted(
        &self,
        mouse: &Mouse,
        mods: ModifiersState,
        mouse_mode: bool,
        selection: bool,
    ) -> Option<DisplayTxtObj> {
        // Make sure all prerequisites for highlighting are met
        if selection
            || (mouse_mode && !mods.shift())
            || !mouse.inside_grid
            || mouse.left_button_state == ElementState::Pressed
        {
            return None;
        }

        let mut hovers: Vec<_> = self
            .display_objects
            .iter()
            .filter(|ob| (ob.start..=ob.end).contains(&Point::new(mouse.line, mouse.column)))
            .collect();
        hovers.sort_by(|txob_a, txob_b| txob_a.priority.cmp(&txob_b.priority));
        hovers.iter().cloned().next().cloned()
    }
}
