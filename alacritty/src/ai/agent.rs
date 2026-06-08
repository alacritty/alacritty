//! The AI worker thread and its agentic loop.
//!
//! The worker owns the conversation with the model. It runs on a background thread so the
//! blocking HTTP calls never stall the UI. It reads the terminal (to give the model
//! context and to capture command output) and writes commands to the PTY, pushing results
//! back to the main event loop as [`AiEvent`]s.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use regex_automata::meta::Regex;
use serde_json::{Value, json};
use winit::event_loop::EventLoopProxy;
use winit::window::WindowId;

use alacritty_terminal::event_loop::{EventLoopSender, Msg};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;

use crate::ai::approval::{ApprovalPolicy, Decision};
use crate::ai::client::{Client, Message};
use crate::config::ai::Ai;
use crate::event::{Event, EventProxy, EventType};

/// Maximum number of tool-call rounds for a single user prompt, to bound runaway loops.
const MAX_ROUNDS: usize = 16;

/// Upper bound on how long to wait for a command's output to settle.
const MAX_OUTPUT_WAIT: Duration = Duration::from_secs(20);

/// System prompt establishing the assistant's role and tools.
const SYSTEM_PROMPT: &str = "\
You are an AI assistant embedded in the user's terminal emulator. You can see the current \
terminal screen and run shell commands on the user's behalf using the `run_command` tool. \
Be concise. Briefly say what you are about to do, then use the tool. After commands run, \
read their output and continue until the user's request is complete, then give a short \
summary. Prefer non-interactive flags (e.g. `--noconfirm`, `-y`) to avoid blocking on \
confirmation prompts. If a command may prompt for input such as a password (e.g. `sudo`) or \
a yes/no confirmation, set `interactive` to true so the user can respond in the terminal. \
Never include secrets in your replies.";

/// Message sent from the UI to the worker.
#[derive(Debug)]
pub enum WorkerMsg {
    /// A new user prompt.
    Prompt(String),
    /// Approve the destructive command currently awaiting confirmation.
    Approve,
    /// Deny the destructive command currently awaiting confirmation.
    Deny,
    /// Stop the worker thread.
    Shutdown,
}

/// Event sent from the worker back to the main event loop.
#[derive(Debug, Clone)]
pub enum AiEvent {
    /// Set or clear the in-flight indicator.
    Busy(bool),
    /// Set or clear the transient status line.
    Status(Option<String>),
    /// A final assistant text reply to append to the transcript.
    Assistant(String),
    /// A system/status note (e.g. which command is running).
    System(String),
    /// A destructive command is awaiting user confirmation.
    AwaitingApproval { command: String },
    /// A running command is blocked on an interactive prompt; hand control to the user.
    AwaitingInput { prompt: String },
    /// The interactive prompt was resolved; the agent has resumed.
    InputResolved,
    /// An error to surface to the user.
    Error(String),
}

/// Everything the worker needs to run.
pub struct WorkerConfig {
    pub ai: Ai,
    pub api_key: String,
    pub terminal: Arc<FairMutex<Term<EventProxy>>>,
    pub pty: EventLoopSender,
    pub proxy: EventLoopProxy<Event>,
    pub window_id: WindowId,
}

/// Handle to the running worker thread.
pub struct AiWorker {
    tx: Sender<WorkerMsg>,
    _join: JoinHandle<()>,
}

impl AiWorker {
    /// Spawn the worker thread.
    pub fn spawn(config: WorkerConfig) -> Self {
        let (tx, rx) = channel();
        let join = thread::Builder::new()
            .name("ai-worker".into())
            .spawn(move || Worker::new(config).run(rx))
            .expect("spawn ai worker thread");

        Self { tx, _join: join }
    }

    /// Send a message to the worker.
    pub fn send(&self, msg: WorkerMsg) {
        let _ = self.tx.send(msg);
    }
}

impl Drop for AiWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(WorkerMsg::Shutdown);
    }
}

/// Worker state held on the background thread.
struct Worker {
    config: WorkerConfig,
    client: Client,
    policy: ApprovalPolicy,
    prompt_detector: Regex,
    tools: Vec<Value>,
    messages: Vec<Message>,
}

impl Worker {
    fn new(config: WorkerConfig) -> Self {
        let client =
            Client::new(&config.ai.base_url, &config.ai.model, config.api_key.clone());
        let policy = ApprovalPolicy::new(&config.ai.auto_approve, &config.ai.deny);
        let messages = vec![Message::system(SYSTEM_PROMPT)];

        Self {
            config,
            client,
            policy,
            prompt_detector: build_prompt_detector(),
            tools: vec![run_command_tool()],
            messages,
        }
    }

    fn run(mut self, rx: Receiver<WorkerMsg>) {
        while let Ok(msg) = rx.recv() {
            match msg {
                WorkerMsg::Prompt(prompt) => self.handle_prompt(prompt, &rx),
                WorkerMsg::Shutdown => break,
                // Approve/Deny are only meaningful while awaiting approval (handled inline).
                WorkerMsg::Approve | WorkerMsg::Deny => {},
            }
        }
    }

    /// Run the agentic loop for a single user prompt.
    fn handle_prompt(&mut self, prompt: String, rx: &Receiver<WorkerMsg>) {
        self.emit(AiEvent::Busy(true));
        self.emit(AiEvent::Status(None));

        let context = self.snapshot(self.config.ai.context_scrollback_lines);
        self.messages.push(Message::user(format!(
            "Current terminal screen:\n```\n{context}\n```\n\nRequest: {prompt}"
        )));

        for _ in 0..MAX_ROUNDS {
            let reply = match self.client.chat(&self.messages, &self.tools) {
                Ok(reply) => reply,
                Err(err) => {
                    self.emit(AiEvent::Error(err.to_string()));
                    break;
                },
            };

            self.messages.push(reply.clone());

            if reply.tool_calls.is_empty() {
                if let Some(content) = reply.content.filter(|c| !c.trim().is_empty()) {
                    self.emit(AiEvent::Assistant(content));
                }
                break;
            }

            // Execute every requested tool call and feed the results back.
            let mut stop = false;
            for call in &reply.tool_calls {
                let result = match self.execute_tool(call, rx) {
                    ToolOutcome::Result(text) => text,
                    ToolOutcome::Inserted(text) => {
                        // Keep the history well-formed, then end the turn after one insert.
                        self.messages.push(Message::tool_result(call.id.clone(), text));
                        stop = true;
                        break;
                    },
                    ToolOutcome::Aborted => {
                        stop = true;
                        break;
                    },
                };
                self.messages.push(Message::tool_result(call.id.clone(), result));
            }

            if stop {
                break;
            }
        }

        self.emit(AiEvent::Busy(false));
    }

    /// Run one tool call, applying the approval policy.
    fn execute_tool(
        &self,
        call: &crate::ai::client::ToolCall,
        rx: &Receiver<WorkerMsg>,
    ) -> ToolOutcome {
        if call.function.name != "run_command" {
            return ToolOutcome::Result(format!("unsupported tool: {}", call.function.name));
        }

        let args = match parse_command(&call.function.arguments) {
            Some(args) => args,
            None => return ToolOutcome::Result("invalid tool arguments".into()),
        };
        let command = args.command;

        match self.policy.decide(self.config.ai.execution_mode, &command) {
            Decision::Insert => {
                self.write_pty(&command, false);
                self.emit(AiEvent::System(format!("inserted (press Enter to run): {command}")));
                ToolOutcome::Inserted(
                    "The command was inserted at the prompt for the user to run manually. Its \
                     output is not yet available."
                        .into(),
                )
            },
            Decision::Run => ToolOutcome::Result(self.run_and_capture(&command, args.interactive)),
            Decision::Confirm => {
                self.emit(AiEvent::AwaitingApproval { command: command.clone() });
                match wait_for_decision(rx) {
                    Some(true) => {
                        ToolOutcome::Result(self.run_and_capture(&command, args.interactive))
                    },
                    Some(false) => {
                        self.emit(AiEvent::System(format!("declined: {command}")));
                        ToolOutcome::Result("The user declined to run this command.".into())
                    },
                    None => ToolOutcome::Aborted,
                }
            },
        }
    }

    /// Run a command and return the resulting terminal output.
    ///
    /// If the command blocks on an interactive prompt (e.g. a `sudo` password or a `[Y/n]`
    /// confirmation), control is handed to the user via [`AiEvent::AwaitingInput`] until the
    /// prompt is resolved, then the agent resumes automatically.
    fn run_and_capture(&self, command: &str, interactive: bool) -> String {
        self.emit(AiEvent::System(format!("running: {command}")));
        self.write_pty(command, true);
        self.wait_for_completion(interactive);
        let output = self.snapshot(0);
        if output.trim().is_empty() {
            "(command produced no visible output)".into()
        } else {
            output
        }
    }

    /// Wait for a command to finish, handing control to the user on interactive prompts.
    fn wait_for_completion(&self, interactive: bool) {
        // Interactive commands (installs, sudo) get a longer settle window and overall budget.
        let idle = Duration::from_millis(if interactive {
            1500
        } else {
            self.config.ai.output_idle_ms.max(50)
        });
        let overall = if interactive { Duration::from_secs(600) } else { MAX_OUTPUT_WAIT };
        let deadline = Instant::now() + overall;

        let mut handed_off = false;
        let mut last_prompt: Option<String> = None;

        loop {
            self.wait_idle(idle, deadline);
            if Instant::now() >= deadline {
                break;
            }

            let screen = self.snapshot(0);
            match self.detect_prompt(&screen) {
                Some(prompt) => {
                    // Announce a newly-seen prompt and hand keyboard control to the user.
                    if last_prompt.as_deref() != Some(prompt.as_str()) {
                        self.emit(AiEvent::AwaitingInput { prompt: prompt.clone() });
                        last_prompt = Some(prompt);
                        handed_off = true;
                    }
                    // Wait for the user to act (the screen changes), then re-evaluate.
                    if !self.wait_for_change(&screen, deadline) {
                        break;
                    }
                },
                // No interactive prompt visible: the command has finished (as best we can tell).
                None => break,
            }
        }

        if handed_off {
            self.emit(AiEvent::InputResolved);
        }
    }

    /// Write bytes to the PTY, optionally terminating with a carriage return to execute.
    ///
    /// When executing, the line is cleared first (Ctrl-U) so any residual prompt input does
    /// not concatenate with the injected command.
    fn write_pty(&self, command: &str, execute: bool) {
        let mut bytes = Vec::with_capacity(command.len() + 2);
        if execute {
            bytes.push(0x15); // Ctrl-U: kill line (clear any partial prompt input).
        }
        bytes.extend_from_slice(command.as_bytes());
        if execute {
            bytes.push(b'\r');
        }
        let _ = self.config.pty.send(Msg::Input(bytes.into()));
    }

    /// Block until terminal output has been quiet for `idle`, or `deadline` passes.
    fn wait_idle(&self, idle: Duration, deadline: Instant) {
        let poll = Duration::from_millis(50);

        let mut last = self.snapshot(0);
        let mut stable_since = Instant::now();

        loop {
            thread::sleep(poll);
            let current = self.snapshot(0);

            if current != last {
                last = current;
                stable_since = Instant::now();
            } else if stable_since.elapsed() >= idle {
                return;
            }

            if Instant::now() >= deadline {
                return;
            }
        }
    }

    /// Poll until the screen differs from `prev`, or `deadline` passes.
    ///
    /// Returns whether the screen changed (a hidden password shows no change until Enter is
    /// pressed, which is exactly when we want to resume).
    fn wait_for_change(&self, prev: &str, deadline: Instant) -> bool {
        let poll = Duration::from_millis(80);
        loop {
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(poll);
            if self.snapshot(0) != prev {
                return true;
            }
        }
    }

    /// Detect an interactive prompt on the last non-blank line of `screen`.
    fn detect_prompt(&self, screen: &str) -> Option<String> {
        let last = screen.lines().rev().find(|line| !line.trim().is_empty())?;
        self.prompt_detector.is_match(last).then(|| last.trim().to_owned())
    }

    /// Capture the visible screen plus `scrollback` lines of history as text.
    fn snapshot(&self, scrollback: usize) -> String {
        let term = self.config.terminal.lock();
        let display_offset = term.grid().display_offset() as i32;
        let extra = scrollback.min(term.history_size()) as i32;

        let top = (-display_offset - extra).max(term.topmost_line().0);
        let bottom = (term.bottommost_line().0 - display_offset).max(top);

        let start = Point::new(Line(top), Column(0));
        let end = Point::new(Line(bottom), term.last_column());
        let text = term.bounds_to_string(start, end);
        drop(term);

        // Trim trailing blank lines for a tighter context.
        text.trim_end().to_owned()
    }

    /// Send an event back to the main loop for this window.
    fn emit(&self, event: AiEvent) {
        let _ = self
            .config
            .proxy
            .send_event(Event::new(EventType::Ai(event), Some(self.config.window_id)));
    }
}

/// Outcome of executing a single tool call.
enum ToolOutcome {
    /// The tool produced a result to feed back to the model; continue the loop.
    Result(String),
    /// The command was inserted for the user to run (TypeOnly). End the turn — nothing ran,
    /// so there is no output to feed back and looping would just re-insert it.
    Inserted(String),
    /// The worker is shutting down.
    Aborted,
}

/// Block waiting for an approval decision, ignoring stray prompts.
///
/// Returns `Some(true)` to approve, `Some(false)` to deny, or `None` to abort (shutdown).
fn wait_for_decision(rx: &Receiver<WorkerMsg>) -> Option<bool> {
    loop {
        match rx.recv_timeout(Duration::from_secs(3600)) {
            Ok(WorkerMsg::Approve) => return Some(true),
            Ok(WorkerMsg::Deny) => return Some(false),
            Ok(WorkerMsg::Shutdown) | Err(RecvTimeoutError::Disconnected) => return None,
            // Ignore prompts and timeouts while awaiting a decision.
            Ok(WorkerMsg::Prompt(_)) | Err(RecvTimeoutError::Timeout) => {},
        }
    }
}

/// Parsed `run_command` tool arguments.
struct ToolArgs {
    command: String,
    interactive: bool,
}

/// Parse the `command` (and optional `interactive`) fields from a tool call's arguments.
fn parse_command(arguments: &str) -> Option<ToolArgs> {
    let value: Value = serde_json::from_str(arguments).ok()?;
    let command = value.get("command")?.as_str()?.to_owned();
    let interactive = value.get("interactive").and_then(Value::as_bool).unwrap_or(false);
    Some(ToolArgs { command, interactive })
}

/// The OpenAI tool definition for `run_command`.
fn run_command_tool() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "run_command",
            "description": "Run a shell command in the user's terminal and return its output. \
                            Use this to inspect the system and perform requested tasks.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to run."
                    },
                    "interactive": {
                        "type": "boolean",
                        "description": "Set true if the command may prompt for input (e.g. a \
                                        sudo password or a yes/no confirmation), so the user \
                                        can respond in the terminal."
                    }
                },
                "required": ["command"]
            }
        }
    })
}

/// Patterns matched against the last terminal line to detect an interactive prompt.
const PROMPT_PATTERNS: &[&str] = &[
    r"(?i)password[^:]*:\s*$",            // sudo / ssh / generic password
    r"\[sudo\]",                          // sudo lecture line
    r"(?i)\[y/n\]\s*$",                   // [Y/n] / [y/N]
    r"(?i)\(yes/no(/\[fingerprint\])?\)", // ssh host key
    r"(?i)\(y/n\)\s*$",
    r"(?i)proceed with",                  // pacman ":: Proceed with installation?"
    r"(?i)continue connecting",           // ssh
    r"(?i)\?\s*\[[yn]",                   // "...? [Y/n]" / "...? [y/N]"
    r"(?i)overwrite\b.*\?",
];

/// Compile the interactive-prompt detector.
fn build_prompt_detector() -> Regex {
    Regex::new_many(PROMPT_PATTERNS).expect("built-in prompt patterns must compile")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_extracts_fields() {
        let args = parse_command(r#"{"command":"ls -la"}"#).unwrap();
        assert_eq!(args.command, "ls -la");
        assert!(!args.interactive);

        let args = parse_command(r#"{"command":"sudo pacman -S docker","interactive":true}"#)
            .unwrap();
        assert_eq!(args.command, "sudo pacman -S docker");
        assert!(args.interactive);

        assert!(parse_command("not json").is_none());
        assert!(parse_command(r#"{"other":1}"#).is_none());
    }

    #[test]
    fn detects_interactive_prompts() {
        let re = build_prompt_detector();
        let prompt = |s: &str| {
            let last = s.lines().rev().find(|l| !l.trim().is_empty()).unwrap();
            re.is_match(last)
        };
        assert!(prompt("[sudo] password for mike: "));
        assert!(prompt("Password:"));
        assert!(prompt(":: Proceed with installation? [Y/n] "));
        assert!(prompt("Overwrite existing file? [y/N]"));
        assert!(prompt("Are you sure you want to continue connecting (yes/no)? "));
        // Normal output / shell prompts must not match.
        assert!(!prompt("total 48"));
        assert!(!prompt("mike@host /tmp $ "));
        assert!(!prompt("Reading package lists... Done"));
    }

    #[test]
    fn tool_definition_is_well_formed() {
        let tool = run_command_tool();
        assert_eq!(tool["function"]["name"], "run_command");
        assert_eq!(tool["type"], "function");
    }
}
