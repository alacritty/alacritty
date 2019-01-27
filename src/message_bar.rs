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

use std::sync::{Arc, Mutex};

use crate::Rgb;

#[derive(Debug, Eq, PartialEq, Clone)]
struct Message {
    message: String,
    color: Rgb,
    id: usize,
}

impl Message {
    fn new(message: String, color: Rgb, id: usize) -> Message {
        Message { message, color, id }
    }
}

#[derive(Debug, Clone)]
pub struct MessageBar {
    inner: Arc<Mutex<SharedMessageBar>>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct SharedMessageBar {
    messages: Vec<Message>,
    id: usize,
}

impl MessageBar {
    pub fn new() -> MessageBar {
        MessageBar {
            inner: Arc::new(Mutex::new(SharedMessageBar {
                messages: Vec::new(),
                id: 0,
            })),
        }
    }

    pub fn is_empty(&self) -> bool {
        let lock = self.inner.lock().unwrap();
        lock.messages.is_empty()
    }

    pub fn message(&self) -> String {
        let lock = self.inner.lock().unwrap();
        let len = lock.messages.len();
        lock.messages[len - 1].message.clone()
    }

    pub fn color(&self) -> Rgb {
        let lock = self.inner.lock().unwrap();
        let len = lock.messages.len();
        lock.messages[len - 1].color
    }

    pub fn push(&mut self, message: String, color: Rgb) -> usize {
        let mut lock = self.inner.lock().unwrap();
        lock.id += 1;
        let id = lock.id;
        lock.messages.push(Message::new(message, color, id));
        id
    }

    pub fn pop(&mut self) {
        let mut lock = self.inner.lock().unwrap();
        lock.messages.pop();
    }

    pub fn remove(&mut self, id: usize) {
        let mut lock = self.inner.lock().unwrap();
        lock.messages.retain(|msg| msg.id != id);
    }
}
