// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crossbeam_channel::{Sender, Receiver};

use crate::term::color::Rgb;
use crate::term::SizeInfo;

pub const CLOSE_BUTTON_TEXT: &str = "[X]";
const MIN_FREE_LINES: usize = 3;
const TRUNCATED_MESSAGE: &str = "[MESSAGE TRUNCATED]";

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Message {
    text: String,
    color: Rgb,
}

impl Message {
    pub fn new(text: String, color: Rgb) -> Message {
        Message { text, color }
    }

    pub fn text(&self, size_info: &SizeInfo) -> Vec<String> {
        let num_cols = size_info.cols().0;
        let max_lines = size_info.lines().saturating_sub(MIN_FREE_LINES);

        // Split line to fit the screen
        let mut lines = Vec::new();
        let mut line = String::new();
        for c in self.text.trim().chars() {
            if c == '\n' || line.len() == num_cols {
                lines.push(Self::pad_text(line, num_cols));
                line = String::new();
            }

            if c != '\n' {
                line.push(c);
            }
        }
        lines.push(Self::pad_text(line, num_cols));

        // Truncate output if it's too long
        if lines.len() > max_lines {
            lines.truncate(max_lines);
            if TRUNCATED_MESSAGE.len() <= num_cols {
                if let Some(line) = lines.iter_mut().last() {
                    *line = Self::pad_text(TRUNCATED_MESSAGE.into(), num_cols);
                }
            }
        }

        // Append close button to first line
        if CLOSE_BUTTON_TEXT.len() <= num_cols {
            if let Some(line) = lines.get_mut(0) {
                line.truncate(num_cols - CLOSE_BUTTON_TEXT.len());
                line.push_str(CLOSE_BUTTON_TEXT);
            }
        }

        lines
    }

    pub fn color(&self) -> Rgb {
        self.color
    }

    fn pad_text(mut text: String, num_cols: usize) -> String {
        let padding_len = num_cols.saturating_sub(text.len());
        text.extend(vec![' '; padding_len]);
        text
    }
}

#[derive(Debug)]
pub struct MessageBar {
    current: Option<Message>,
    messages: Receiver<Message>,
    tx: Sender<Message>,
}

impl MessageBar {
    pub fn new() -> MessageBar {
        let (tx, messages) = crossbeam_channel::unbounded();
        MessageBar {
            current: None,
            messages,
            tx,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.current.is_none()
    }

    pub fn message(&mut self) -> Option<Message> {
        if let Some(current) = &self.current {
            Some(current.clone())
        } else {
            self.current = self.messages.try_recv().ok();
            self.current.clone()
        }
    }

    pub fn tx(&self) -> Sender<Message> {
        self.tx.clone()
    }

    pub fn pop(&mut self) {
        self.current = self.messages.try_recv().ok();
    }
}

impl Default for MessageBar {
    fn default() -> MessageBar {
        MessageBar::new()
    }
}

#[cfg(test)]
mod test {
    use super::{Message, MessageBar, MIN_FREE_LINES};
    use crate::term::{SizeInfo, color};

    #[test]
    fn appends_close_button() {
        let input = "test";
        let mut message_bar = MessageBar::new();
        message_bar.tx().send(Message::new(input.into(), color::RED)).unwrap();
        let size = SizeInfo {
            width: 7.,
            height: 10.,
            cell_width: 1.,
            cell_height: 1.,
            padding_x: 0.,
            padding_y: 0.,
            dpr: 0.,
        };

        let lines = message_bar.message().unwrap().text(&size);

        assert_eq!(lines, vec![String::from("test[X]")]);
    }

    #[test]
    fn multiline_appends_close_button() {
        let input = "foo\nbar";
        let mut message_bar = MessageBar::new();
        message_bar.tx().send(Message::new(input.into(), color::RED)).unwrap();
        let size = SizeInfo {
            width: 6.,
            height: 10.,
            cell_width: 1.,
            cell_height: 1.,
            padding_x: 0.,
            padding_y: 0.,
            dpr: 0.,
        };

        let lines = message_bar.message().unwrap().text(&size);

        assert_eq!(lines, vec![String::from("foo[X]"), String::from("bar   ")]);
    }

    #[test]
    fn splits_on_newline() {
        let input = "foo\nbar";
        let mut message_bar = MessageBar::new();
        message_bar.tx().send(Message::new(input.into(), color::RED)).unwrap();
        let size = SizeInfo {
            width: 6.,
            height: 10.,
            cell_width: 1.,
            cell_height: 1.,
            padding_x: 0.,
            padding_y: 0.,
            dpr: 0.,
        };

        let lines = message_bar.message().unwrap().text(&size);

        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn splits_on_length() {
        let input = "foobar123";
        let mut message_bar = MessageBar::new();
        message_bar.tx().send(Message::new(input.into(), color::RED)).unwrap();
        let size = SizeInfo {
            width: 6.,
            height: 10.,
            cell_width: 1.,
            cell_height: 1.,
            padding_x: 0.,
            padding_y: 0.,
            dpr: 0.,
        };

        let lines = message_bar.message().unwrap().text(&size);

        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn empty_with_shortterm() {
        let input = "foobar";
        let mut message_bar = MessageBar::new();
        message_bar.tx().send(Message::new(input.into(), color::RED)).unwrap();
        let size = SizeInfo {
            width: 6.,
            height: 0.,
            cell_width: 1.,
            cell_height: 1.,
            padding_x: 0.,
            padding_y: 0.,
            dpr: 0.,
        };

        let lines = message_bar.message().unwrap().text(&size);

        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn truncates_long_messages() {
        let input = "hahahahahahahahahahaha truncate this because it's too long for the term";
        let mut message_bar = MessageBar::new();
        message_bar.tx().send(Message::new(input.into(), color::RED)).unwrap();
        let size = SizeInfo {
            width: 22.,
            height: (MIN_FREE_LINES + 2) as f32,
            cell_width: 1.,
            cell_height: 1.,
            padding_x: 0.,
            padding_y: 0.,
            dpr: 0.,
        };

        let lines = message_bar.message().unwrap().text(&size);

        assert_eq!(
            lines,
            vec![String::from("hahahahahahahahahah[X]"), String::from("[MESSAGE TRUNCATED]   ")]
        );
    }

    #[test]
    fn hide_button_when_too_narrow() {
        let input = "ha";
        let mut message_bar = MessageBar::new();
        message_bar.tx().send(Message::new(input.into(), color::RED)).unwrap();
        let size = SizeInfo {
            width: 2.,
            height: 10.,
            cell_width: 1.,
            cell_height: 1.,
            padding_x: 0.,
            padding_y: 0.,
            dpr: 0.,
        };

        let lines = message_bar.message().unwrap().text(&size);

        assert_eq!(lines, vec![String::from("ha")]);
    }

    #[test]
    fn hide_truncated_when_too_narrow() {
        let input = "hahahahahahahahaha";
        let mut message_bar = MessageBar::new();
        message_bar.tx().send(Message::new(input.into(), color::RED)).unwrap();
        let size = SizeInfo {
            width: 2.,
            height: (MIN_FREE_LINES + 2) as f32,
            cell_width: 1.,
            cell_height: 1.,
            padding_x: 0.,
            padding_y: 0.,
            dpr: 0.,
        };

        let lines = message_bar.message().unwrap().text(&size);

        assert_eq!(lines, vec![String::from("ha"), String::from("ha")]);
    }

    #[test]
    fn replace_message_for_button() {
        let input = "test";
        let mut message_bar = MessageBar::new();
        message_bar.tx().send(Message::new(input.into(), color::RED)).unwrap();
        let size = SizeInfo {
            width: 5.,
            height: 10.,
            cell_width: 1.,
            cell_height: 1.,
            padding_x: 0.,
            padding_y: 0.,
            dpr: 0.,
        };

        let lines = message_bar.message().unwrap().text(&size);

        assert_eq!(lines, vec![String::from("te[X]")]);
    }
}
