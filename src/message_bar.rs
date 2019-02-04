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

use crossbeam_channel::{Receiver, Sender};

use crate::term::color::Rgb;
use crate::term::SizeInfo;

pub const CLOSE_BUTTON_TEXT: &str = " [X]";
const MIN_FREE_LINES: usize = 3;
const TRUNCATED_MESSAGE: &str = "[MESSAGE TRUNCATED]";

/// Message for display in the MessageBar
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Message {
    text: String,
    color: Rgb,
    topic: Option<String>,
}

impl Message {
    /// Create a new message
    pub fn new(text: String, color: Rgb) -> Message {
        Message {
            text,
            color,
            topic: None,
        }
    }

    /// Formatted message text lines
    pub fn text(&self, size_info: &SizeInfo) -> Vec<String> {
        let num_cols = size_info.cols().0;
        let max_lines = size_info.lines().saturating_sub(MIN_FREE_LINES);

        // Split line to fit the screen
        let mut lines = Vec::new();
        let mut line = String::new();
        for c in self.text.trim().chars() {
            if c == '\n'
                || line.len() == num_cols
                // Keep space in first line for button
                || (lines.is_empty()
                    && num_cols >= CLOSE_BUTTON_TEXT.len()
                    && line.len() == num_cols.saturating_sub(CLOSE_BUTTON_TEXT.len()))
            {
                // Attempt to wrap on word boundaries
                if let (Some(index), true) = (line.rfind(char::is_whitespace), c != '\n') {
                    let split = line.split_off(index + 1);
                    line.pop();
                    lines.push(Self::pad_text(line, num_cols));
                    line = split
                } else {
                    lines.push(Self::pad_text(line, num_cols));
                    line = String::new();
                }
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

    /// Message color
    #[inline]
    pub fn color(&self) -> Rgb {
        self.color
    }

    /// Message topic
    #[inline]
    pub fn topic(&self) -> Option<&String> {
        self.topic.as_ref()
    }

    /// Update the message topic
    #[inline]
    pub fn set_topic(&mut self, topic: String) {
        self.topic = Some(topic);
    }

    /// Right-pad text to fit a specific number of columns
    #[inline]
    fn pad_text(mut text: String, num_cols: usize) -> String {
        let padding_len = num_cols.saturating_sub(text.len());
        text.extend(vec![' '; padding_len]);
        text
    }
}

/// Storage for message bar
#[derive(Debug)]
pub struct MessageBar {
    current: Option<Message>,
    messages: Receiver<Message>,
    tx: Sender<Message>,
}

impl MessageBar {
    /// Create new message bar
    pub fn new() -> MessageBar {
        let (tx, messages) = crossbeam_channel::unbounded();
        MessageBar {
            current: None,
            messages,
            tx,
        }
    }

    /// Check if there are any messages queued
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.current.is_none()
    }

    /// Current message
    #[inline]
    pub fn message(&mut self) -> Option<Message> {
        if let Some(current) = &self.current {
            Some(current.clone())
        } else {
            self.current = self.messages.try_recv().ok();
            self.current.clone()
        }
    }

    /// Channel for adding new messages
    #[inline]
    pub fn tx(&self) -> Sender<Message> {
        self.tx.clone()
    }

    /// Remove the currently visible message
    #[inline]
    pub fn pop(&mut self) -> Option<Message> {
        std::mem::replace(&mut self.current, self.messages.try_recv().ok())
    }

    /// Remove all messages with a specific topic
    #[inline]
    pub fn remove_topic(&mut self, topic: String) {
        // Remove the currently active message
        while self.current.as_ref().and_then(|m| m.topic()) == Some(&topic) {
            self.pop();
        }

        // Filter messages currently pending
        for msg in self
            .messages
            .try_iter()
            .take(self.messages.len())
            .filter(|m| m.topic() != Some(&topic))
        {
            let _ = self.tx.send(msg);
        }
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
    use crate::term::{color, SizeInfo};

    #[test]
    fn appends_close_button() {
        let input = "a";
        let mut message_bar = MessageBar::new();
        message_bar
            .tx()
            .send(Message::new(input.into(), color::RED))
            .unwrap();
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

        assert_eq!(lines, vec![String::from("a   [X]")]);
    }

    #[test]
    fn multiline_close_button_first_line() {
        let input = "fo\nbar";
        let mut message_bar = MessageBar::new();
        message_bar
            .tx()
            .send(Message::new(input.into(), color::RED))
            .unwrap();
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

        assert_eq!(lines, vec![String::from("fo [X]"), String::from("bar   ")]);
    }

    #[test]
    fn splits_on_newline() {
        let input = "a\nb";
        let mut message_bar = MessageBar::new();
        message_bar
            .tx()
            .send(Message::new(input.into(), color::RED))
            .unwrap();
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
        let input = "foobar1";
        let mut message_bar = MessageBar::new();
        message_bar
            .tx()
            .send(Message::new(input.into(), color::RED))
            .unwrap();
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
        message_bar
            .tx()
            .send(Message::new(input.into(), color::RED))
            .unwrap();
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
        message_bar
            .tx()
            .send(Message::new(input.into(), color::RED))
            .unwrap();
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
            vec![
                String::from("hahahahahahahahaha [X]"),
                String::from("[MESSAGE TRUNCATED]   ")
            ]
        );
    }

    #[test]
    fn hide_button_when_too_narrow() {
        let input = "ha";
        let mut message_bar = MessageBar::new();
        message_bar
            .tx()
            .send(Message::new(input.into(), color::RED))
            .unwrap();
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
        message_bar
            .tx()
            .send(Message::new(input.into(), color::RED))
            .unwrap();
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
    fn add_newline_for_button() {
        let input = "test";
        let mut message_bar = MessageBar::new();
        message_bar
            .tx()
            .send(Message::new(input.into(), color::RED))
            .unwrap();
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

        assert_eq!(lines, vec![String::from("t [X]"), String::from("est  ")]);
    }

    #[test]
    fn remove_topic() {
        let mut message_bar = MessageBar::new();
        for i in 0..10 {
            let mut msg = Message::new(String::new(), color::RED);
            if i % 2 == 0 {
                msg.set_topic("topic".into());
            }
            message_bar.tx().send(msg).unwrap();
        }

        message_bar.remove_topic("topic".into());

        // Count number of messages
        message_bar.pop();
        let mut num_messages = 0;
        while message_bar.pop().is_some() {
            num_messages += 1;
        }

        assert_eq!(num_messages, 5);
    }

    #[test]
    fn pop() {
        let mut message_bar = MessageBar::new();
        let one = Message::new(String::from("one"), color::RED);
        message_bar.tx().send(one.clone()).unwrap();
        let two = Message::new(String::from("two"), color::YELLOW);
        message_bar.tx().send(two.clone()).unwrap();

        assert_eq!(message_bar.message(), Some(one));

        message_bar.pop();

        assert_eq!(message_bar.message(), Some(two));
    }

    #[test]
    fn wrap_on_words() {
        let input = "a\nbc defg";
        let mut message_bar = MessageBar::new();
        message_bar
            .tx()
            .send(Message::new(input.into(), color::RED))
            .unwrap();
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

        assert_eq!(lines, vec![String::from("a [X]"), String::from("bc   "), String::from("defg ")]);
    }
}
