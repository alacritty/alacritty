use std::collections::VecDeque;

use alacritty_terminal::term::SizeInfo;

pub const CLOSE_BUTTON_TEXT: &str = "[X]";
const CLOSE_BUTTON_PADDING: usize = 1;
const MIN_FREE_LINES: usize = 3;
const TRUNCATED_MESSAGE: &str = "[MESSAGE TRUNCATED]";

/// Message for display in the MessageBuffer.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Message {
    text: String,
    ty: MessageType,
    target: Option<String>,
}

/// Purpose of the message.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum MessageType {
    /// A message represents an error.
    Error,

    /// A message represents a warning.
    Warning,
}

impl Message {
    /// Create a new message.
    pub fn new(text: String, ty: MessageType) -> Message {
        Message { text, ty, target: None }
    }

    /// Formatted message text lines.
    pub fn text(&self, size_info: &SizeInfo) -> Vec<String> {
        let num_cols = size_info.cols().0;
        let total_lines =
            (size_info.height() - 2. * size_info.padding_y()) / size_info.cell_height();
        let max_lines = (total_lines as usize).saturating_sub(MIN_FREE_LINES);
        let button_len = CLOSE_BUTTON_TEXT.len();

        // Split line to fit the screen.
        let mut lines = Vec::new();
        let mut line = String::new();
        for c in self.text.trim().chars() {
            if c == '\n'
                || line.len() == num_cols
                // Keep space in first line for button.
                || (lines.is_empty()
                    && num_cols >= button_len
                    && line.len() == num_cols.saturating_sub(button_len + CLOSE_BUTTON_PADDING))
            {
                // Attempt to wrap on word boundaries.
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

        // Truncate output if it's too long.
        if lines.len() > max_lines {
            lines.truncate(max_lines);
            if TRUNCATED_MESSAGE.len() <= num_cols {
                if let Some(line) = lines.iter_mut().last() {
                    *line = Self::pad_text(TRUNCATED_MESSAGE.into(), num_cols);
                }
            }
        }

        // Append close button to first line.
        if button_len <= num_cols {
            if let Some(line) = lines.get_mut(0) {
                line.truncate(num_cols - button_len);
                line.push_str(CLOSE_BUTTON_TEXT);
            }
        }

        lines
    }

    /// Message type.
    #[inline]
    pub fn ty(&self) -> MessageType {
        self.ty
    }

    /// Message target.
    #[inline]
    pub fn target(&self) -> Option<&String> {
        self.target.as_ref()
    }

    /// Update the message target.
    #[inline]
    pub fn set_target(&mut self, target: String) {
        self.target = Some(target);
    }

    /// Right-pad text to fit a specific number of columns.
    #[inline]
    fn pad_text(mut text: String, num_cols: usize) -> String {
        let padding_len = num_cols.saturating_sub(text.len());
        text.extend(vec![' '; padding_len]);
        text
    }
}

/// Storage for message bar.
#[derive(Debug, Default)]
pub struct MessageBuffer {
    messages: VecDeque<Message>,
}

impl MessageBuffer {
    /// Create new message buffer.
    pub fn new() -> MessageBuffer {
        MessageBuffer { messages: VecDeque::new() }
    }

    /// Check if there are any messages queued.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Current message.
    #[inline]
    pub fn message(&self) -> Option<&Message> {
        self.messages.front()
    }

    /// Remove the currently visible message.
    #[inline]
    pub fn pop(&mut self) {
        // Remove the message itself.
        let msg = self.messages.pop_front();

        // Remove all duplicates.
        if let Some(msg) = msg {
            self.messages = self.messages.drain(..).filter(|m| m != &msg).collect();
        }
    }

    /// Remove all messages with a specific target.
    #[inline]
    pub fn remove_target(&mut self, target: &str) {
        self.messages = self
            .messages
            .drain(..)
            .filter(|m| m.target().map(String::as_str) != Some(target))
            .collect();
    }

    /// Add a new message to the queue.
    #[inline]
    pub fn push(&mut self, message: Message) {
        self.messages.push_back(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use alacritty_terminal::term::SizeInfo;

    #[test]
    fn appends_close_button() {
        let input = "a";
        let mut message_buffer = MessageBuffer::new();
        message_buffer.push(Message::new(input.into(), MessageType::Error));
        let size = SizeInfo::new(7., 10., 1., 1., 0., 0., false);

        let lines = message_buffer.message().unwrap().text(&size);

        assert_eq!(lines, vec![String::from("a   [X]")]);
    }

    #[test]
    fn multiline_close_button_first_line() {
        let input = "fo\nbar";
        let mut message_buffer = MessageBuffer::new();
        message_buffer.push(Message::new(input.into(), MessageType::Error));
        let size = SizeInfo::new(6., 10., 1., 1., 0., 0., false);

        let lines = message_buffer.message().unwrap().text(&size);

        assert_eq!(lines, vec![String::from("fo [X]"), String::from("bar   ")]);
    }

    #[test]
    fn splits_on_newline() {
        let input = "a\nb";
        let mut message_buffer = MessageBuffer::new();
        message_buffer.push(Message::new(input.into(), MessageType::Error));
        let size = SizeInfo::new(6., 10., 1., 1., 0., 0., false);

        let lines = message_buffer.message().unwrap().text(&size);

        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn splits_on_length() {
        let input = "foobar1";
        let mut message_buffer = MessageBuffer::new();
        message_buffer.push(Message::new(input.into(), MessageType::Error));
        let size = SizeInfo::new(6., 10., 1., 1., 0., 0., false);

        let lines = message_buffer.message().unwrap().text(&size);

        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn empty_with_shortterm() {
        let input = "foobar";
        let mut message_buffer = MessageBuffer::new();
        message_buffer.push(Message::new(input.into(), MessageType::Error));
        let size = SizeInfo::new(6., 0., 1., 1., 0., 0., false);

        let lines = message_buffer.message().unwrap().text(&size);

        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn truncates_long_messages() {
        let input = "hahahahahahahahahahaha truncate this because it's too long for the term";
        let mut message_buffer = MessageBuffer::new();
        message_buffer.push(Message::new(input.into(), MessageType::Error));
        let size = SizeInfo::new(22., (MIN_FREE_LINES + 2) as f32, 1., 1., 0., 0., false);

        let lines = message_buffer.message().unwrap().text(&size);

        assert_eq!(lines, vec![
            String::from("hahahahahahahahaha [X]"),
            String::from("[MESSAGE TRUNCATED]   ")
        ]);
    }

    #[test]
    fn hide_button_when_too_narrow() {
        let input = "ha";
        let mut message_buffer = MessageBuffer::new();
        message_buffer.push(Message::new(input.into(), MessageType::Error));
        let size = SizeInfo::new(2., 10., 1., 1., 0., 0., false);

        let lines = message_buffer.message().unwrap().text(&size);

        assert_eq!(lines, vec![String::from("ha")]);
    }

    #[test]
    fn hide_truncated_when_too_narrow() {
        let input = "hahahahahahahahaha";
        let mut message_buffer = MessageBuffer::new();
        message_buffer.push(Message::new(input.into(), MessageType::Error));
        let size = SizeInfo::new(2., (MIN_FREE_LINES + 2) as f32, 1., 1., 0., 0., false);

        let lines = message_buffer.message().unwrap().text(&size);

        assert_eq!(lines, vec![String::from("ha"), String::from("ha")]);
    }

    #[test]
    fn add_newline_for_button() {
        let input = "test";
        let mut message_buffer = MessageBuffer::new();
        message_buffer.push(Message::new(input.into(), MessageType::Error));
        let size = SizeInfo::new(5., 10., 1., 1., 0., 0., false);

        let lines = message_buffer.message().unwrap().text(&size);

        assert_eq!(lines, vec![String::from("t [X]"), String::from("est  ")]);
    }

    #[test]
    fn remove_target() {
        let mut message_buffer = MessageBuffer::new();
        for i in 0..10 {
            let mut msg = Message::new(i.to_string(), MessageType::Error);
            if i % 2 == 0 && i < 5 {
                msg.set_target("target".into());
            }
            message_buffer.push(msg);
        }

        message_buffer.remove_target("target");

        // Count number of messages.
        let mut num_messages = 0;
        while message_buffer.message().is_some() {
            num_messages += 1;
            message_buffer.pop();
        }

        assert_eq!(num_messages, 7);
    }

    #[test]
    fn pop() {
        let mut message_buffer = MessageBuffer::new();
        let one = Message::new(String::from("one"), MessageType::Error);
        message_buffer.push(one.clone());
        let two = Message::new(String::from("two"), MessageType::Warning);
        message_buffer.push(two.clone());

        assert_eq!(message_buffer.message(), Some(&one));

        message_buffer.pop();

        assert_eq!(message_buffer.message(), Some(&two));
    }

    #[test]
    fn wrap_on_words() {
        let input = "a\nbc defg";
        let mut message_buffer = MessageBuffer::new();
        message_buffer.push(Message::new(input.into(), MessageType::Error));
        let size = SizeInfo::new(5., 10., 1., 1., 0., 0., false);

        let lines = message_buffer.message().unwrap().text(&size);

        assert_eq!(lines, vec![
            String::from("a [X]"),
            String::from("bc   "),
            String::from("defg ")
        ]);
    }

    #[test]
    fn remove_duplicates() {
        let mut message_buffer = MessageBuffer::new();
        for _ in 0..10 {
            let msg = Message::new(String::from("test"), MessageType::Error);
            message_buffer.push(msg);
        }
        message_buffer.push(Message::new(String::from("other"), MessageType::Error));
        message_buffer.push(Message::new(String::from("test"), MessageType::Warning));
        let _ = message_buffer.message();

        message_buffer.pop();

        // Count number of messages.
        let mut num_messages = 0;
        while message_buffer.message().is_some() {
            num_messages += 1;
            message_buffer.pop();
        }

        assert_eq!(num_messages, 2);
    }
}
