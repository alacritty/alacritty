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

use crate::Rgb;

pub const CLOSE_BUTTON_TEXT: &'static str = "[X]";

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Message {
    text: String,
    color: Rgb,
}

impl Message {
    pub fn new(text: String, color: Rgb) -> Message {
        Message { text, color }
    }

    // TODO: multi-line text
    pub fn text(&self, num_cols: usize) -> Vec<String> {
        let mut text = self.text.clone();

        // Add padding to make the bar take the full width
        let padding_len = num_cols.saturating_sub(text.len() + CLOSE_BUTTON_TEXT.len());
        text.extend(vec![' '; padding_len]);
        text.extend(CLOSE_BUTTON_TEXT.chars());

        vec![text]
    }

    pub fn color(&self) -> Rgb {
        self.color
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
