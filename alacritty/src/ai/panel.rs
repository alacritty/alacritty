//! State and text layout for the AI chat panel.
//!
//! This module owns the panel's data (messages, input buffer, scroll position, approval
//! state) and turns it into wrapped text lines. It performs no rendering itself; the
//! display layer reads [`ChatPanelState`] and draws it with the terminal's glyph renderer.

/// Who produced a chat line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    User,
    Assistant,
    /// System/status notices (errors, command results, hints).
    System,
}

impl Speaker {
    /// Short prefix label shown at the start of a message.
    fn label(self) -> &'static str {
        match self {
            Speaker::User => "you",
            Speaker::Assistant => "ai",
            Speaker::System => "sys",
        }
    }
}

/// A single chat message in the transcript.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub speaker: Speaker,
    pub text: String,
}

/// A destructive command awaiting the user's explicit approval.
#[derive(Debug, Clone)]
pub struct PendingApproval {
    /// The command the model wants to run.
    pub command: String,
}

/// A request the panel wants the window context to forward to the AI worker.
#[derive(Debug, Clone)]
pub enum ChatRequest {
    /// Send a new user prompt to the assistant.
    Prompt(String),
    /// Approve the pending destructive command.
    Approve,
    /// Deny the pending destructive command.
    Deny,
    /// Reset the conversation (start fresh).
    Clear,
}

/// All state for one window's chat panel.
#[derive(Debug, Default)]
pub struct ChatPanelState {
    /// Whether the panel is visible (and thus reserving terminal rows).
    pub open: bool,
    /// Whether keyboard input is routed to the panel rather than the PTY.
    pub focused: bool,
    /// Transcript, oldest first.
    pub messages: Vec<ChatMessage>,
    /// Current input line being composed.
    pub input: String,
    /// Number of wrapped lines scrolled up from the bottom of the transcript.
    pub scroll: usize,
    /// Whether a request to the model is in flight.
    pub busy: bool,
    /// A destructive command awaiting confirmation, if any.
    pub pending_approval: Option<PendingApproval>,
    /// An interactive prompt the running command is blocked on; when set, keyboard input
    /// goes to the terminal so the user can respond (e.g. type a password).
    pub awaiting_input: Option<String>,
    /// Transient status line (e.g. missing API key), shown in the header.
    pub status: Option<String>,
    /// Requests queued for the window context to forward to the AI worker.
    pub outbox: Vec<ChatRequest>,
}

impl ChatPanelState {
    /// Clear the transcript and per-conversation state to start fresh. Visibility/focus
    /// are preserved so the panel stays open and usable.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.input.clear();
        self.scroll = 0;
        self.busy = false;
        self.pending_approval = None;
        self.awaiting_input = None;
        self.status = None;
    }

    /// Toggle visibility. Opening also focuses the panel; closing unfocuses it.
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.focused = self.open;
    }

    /// Append a message to the transcript and stick to the bottom.
    pub fn push(&mut self, speaker: Speaker, text: impl Into<String>) {
        self.messages.push(ChatMessage { speaker, text: text.into() });
        self.scroll = 0;
    }

    /// Append streamed text to the last assistant message, or start a new one.
    ///
    /// Reserved for token streaming; the current client returns complete messages.
    #[allow(dead_code)]
    pub fn append_assistant(&mut self, chunk: &str) {
        match self.messages.last_mut() {
            Some(message) if message.speaker == Speaker::Assistant => message.text.push_str(chunk),
            _ => self.messages.push(ChatMessage {
                speaker: Speaker::Assistant,
                text: chunk.to_owned(),
            }),
        }
        self.scroll = 0;
    }

    /// Scroll the transcript by `delta` wrapped lines (positive scrolls up/back).
    pub fn scroll_by(&mut self, delta: i32, max_scroll: usize) {
        let next = self.scroll as i32 + delta;
        self.scroll = next.clamp(0, max_scroll as i32) as usize;
    }

    /// Render the full transcript into wrapped display lines for the given column width.
    pub fn rendered_lines(&self, columns: usize) -> Vec<String> {
        let width = columns.max(1);
        let mut lines = Vec::new();

        for message in &self.messages {
            let prefix = format!("{:>3} \u{2502} ", message.speaker.label());
            let indent = " ".repeat(prefix.chars().count().min(width.saturating_sub(1)));

            // Preserve explicit newlines, then wrap each paragraph.
            for (i, paragraph) in message.text.split('\n').enumerate() {
                let lead = if i == 0 { &prefix } else { &indent };
                let body_width = width.saturating_sub(lead.chars().count()).max(1);
                let wrapped = wrap(paragraph, body_width);

                if wrapped.is_empty() {
                    lines.push(lead.clone());
                    continue;
                }
                for (j, segment) in wrapped.into_iter().enumerate() {
                    if i == 0 && j == 0 {
                        lines.push(format!("{prefix}{segment}"));
                    } else {
                        lines.push(format!("{indent}{segment}"));
                    }
                }
            }
        }

        lines
    }

    /// Wrap the current input buffer into display lines for the given column width.
    ///
    /// The first line carries the `"> "` prompt; continuation lines are indented to align
    /// under it. Explicit newlines (e.g. from pasted multi-line text) start fresh lines.
    /// This is display-only — `input` is never mutated, so submit still sends the exact
    /// buffer. Always returns at least one line.
    pub fn input_lines(&self, columns: usize) -> Vec<String> {
        let width = columns.max(1);
        let prefix = "> ";
        let indent = " ".repeat(prefix.len());
        let body_width = width.saturating_sub(prefix.chars().count()).max(1);

        let mut lines = Vec::new();
        for (i, paragraph) in self.input.split('\n').enumerate() {
            for segment in wrap(paragraph, body_width) {
                let lead = if lines.is_empty() && i == 0 { prefix } else { &indent };
                lines.push(format!("{lead}{segment}"));
            }
        }
        if lines.is_empty() {
            lines.push(prefix.to_owned());
        }
        lines
    }
}

/// Greedy word-wrap a single line to `width` columns, hard-breaking overlong words.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_len = 0;

    for word in text.split(' ') {
        let word_len = word.chars().count();

        // Hard-break a word that cannot fit on a line by itself.
        if word_len > width {
            if current_len > 0 {
                lines.push(std::mem::take(&mut current));
            }
            let mut chunk = String::new();
            for ch in word.chars() {
                if chunk.chars().count() == width {
                    lines.push(std::mem::take(&mut chunk));
                }
                chunk.push(ch);
            }
            current = chunk;
            current_len = current.chars().count();
            continue;
        }

        let needed = if current_len == 0 { word_len } else { current_len + 1 + word_len };
        if needed > width {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_len = word_len;
        } else {
            if current_len > 0 {
                current.push(' ');
                current_len += 1;
            }
            current.push_str(word);
            current_len += word_len;
        }
    }

    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_breaks_on_words() {
        assert_eq!(wrap("the quick brown fox", 9), vec!["the quick", "brown fox"]);
    }

    #[test]
    fn wrap_hard_breaks_long_words() {
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn input_lines_empty_is_prompt_only() {
        let state = ChatPanelState::default();
        assert_eq!(state.input_lines(40), vec!["> "]);
    }

    #[test]
    fn input_lines_wrap_and_indent() {
        let mut state = ChatPanelState::default();
        // body_width = 6 - 2 = 4; "abcdefghij" hard-breaks into 4-char chunks.
        state.input = "abcdefghij".to_owned();
        assert_eq!(state.input_lines(6), vec!["> abcd", "  efgh", "  ij"]);
    }

    #[test]
    fn input_lines_preserve_newlines() {
        let mut state = ChatPanelState::default();
        state.input = "one\ntwo".to_owned();
        assert_eq!(state.input_lines(40), vec!["> one", "  two"]);
    }

    #[test]
    fn rendered_lines_have_prefix() {
        let mut state = ChatPanelState::default();
        state.push(Speaker::User, "hello");
        let lines = state.rendered_lines(40);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("you \u{2502} hello"));
    }

    #[test]
    fn append_assistant_concatenates() {
        let mut state = ChatPanelState::default();
        state.append_assistant("Hel");
        state.append_assistant("lo");
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].text, "Hello");
    }

    #[test]
    fn toggle_controls_focus() {
        let mut state = ChatPanelState::default();
        assert!(!state.open);
        state.toggle();
        assert!(state.open && state.focused);
        state.toggle();
        assert!(!state.open && !state.focused);
    }
}
