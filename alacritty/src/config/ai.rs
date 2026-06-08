//! Configuration for the AI chat panel.

use serde::Serialize;

use alacritty_config_derive::ConfigDeserialize;

/// Default OpenAI-compatible API base URL.
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Default model used for chat completions.
const DEFAULT_MODEL: &str = "gpt-4o-mini";

/// Default OS-keyring service name used to store the API key.
const DEFAULT_KEYRING_SERVICE: &str = "alacritty-ai";

/// AI chat panel configuration.
///
/// The API key is never stored here; it lives in the OS keyring and is identified by
/// [`Ai::keyring_service`] / [`Ai::keyring_user`].
#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Ai {
    /// Enable the AI chat panel.
    pub enabled: bool,

    /// Base URL of the OpenAI-compatible API (no trailing slash required).
    pub base_url: String,

    /// Model name passed to the chat-completions endpoint.
    pub model: String,

    /// OS-keyring service name under which the API key is stored.
    pub keyring_service: String,

    /// OS-keyring user/account name. When empty, the host of `base_url` is used.
    pub keyring_user: String,

    /// How AI-proposed commands are executed.
    pub execution_mode: ExecutionMode,

    /// Command patterns (regex) that are always auto-approved, even in `Smart` mode.
    pub auto_approve: Vec<String>,

    /// Command patterns (regex) that are always denied/treated as destructive.
    pub deny: Vec<String>,

    /// Height of the chat panel, in terminal lines.
    pub panel_lines: usize,

    /// Number of scrollback lines (above the visible screen) sent to the model as context.
    pub context_scrollback_lines: usize,

    /// Idle window, in milliseconds, used to detect that a command has finished producing
    /// output before its result is captured and fed back to the model.
    pub output_idle_ms: u64,
}

impl Default for Ai {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: DEFAULT_BASE_URL.into(),
            model: DEFAULT_MODEL.into(),
            keyring_service: DEFAULT_KEYRING_SERVICE.into(),
            keyring_user: String::new(),
            execution_mode: ExecutionMode::default(),
            auto_approve: Vec::new(),
            deny: Vec::new(),
            panel_lines: 12,
            context_scrollback_lines: 200,
            output_idle_ms: 400,
        }
    }
}

impl Ai {
    /// Effective keyring user, deriving a value from `base_url` when unset.
    pub fn keyring_user(&self) -> String {
        if !self.keyring_user.is_empty() {
            return self.keyring_user.clone();
        }

        // Derive a stable identifier from the host portion of the base URL.
        self.base_url
            .split("://")
            .nth(1)
            .unwrap_or(&self.base_url)
            .split('/')
            .next()
            .unwrap_or("default")
            .to_owned()
    }
}

/// How commands proposed by the AI are executed.
#[derive(ConfigDeserialize, Serialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Insert the command at the prompt but never run it; the user presses Enter.
    TypeOnly,

    /// Auto-run commands judged safe or matching the allowlist; prompt before destructive ones.
    #[default]
    Smart,

    /// Run every command immediately, without confirmation.
    Yolo,
}
