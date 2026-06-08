//! Minimal OpenAI-compatible chat-completions client.
//!
//! Blocking HTTP via [`ureq`]; intended to be driven from the AI worker thread. The API
//! key is supplied per request and is only ever placed in the `Authorization` header — it
//! is never logged or included in any error returned from this module.

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Request timeout for a single chat-completions call.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Role of a chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A single chat message exchanged with the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    /// Tool calls requested by the assistant (assistant messages only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,

    /// Links a `tool` result back to the assistant's tool call (tool messages only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    /// Build a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::text(Role::System, content)
    }

    /// Build a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::text(Role::User, content)
    }

    /// Build a tool-result message responding to a given tool call id.
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    fn text(role: Role, content: impl Into<String>) -> Self {
        Self { role, content: Some(content.into()), tool_calls: Vec::new(), tool_call_id: None }
    }
}

/// A tool call requested by the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,

    #[serde(rename = "type", default = "default_tool_type")]
    pub kind: String,

    pub function: FunctionCall,
}

fn default_tool_type() -> String {
    "function".to_owned()
}

/// The function portion of a tool call. `arguments` is a JSON-encoded string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Errors returned by the client. None of these ever contain the API key.
#[derive(Debug)]
pub enum ClientError {
    /// Non-2xx HTTP response, with the (key-free) response body.
    Http { status: u16, message: String },
    /// Transport-level failure (DNS, TLS, connection, timeout).
    Transport(String),
    /// Response could not be decoded into the expected shape.
    Decode(String),
    /// The model returned no choices.
    Empty,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http { status, message } => write!(f, "API error (HTTP {status}): {message}"),
            Self::Transport(err) => write!(f, "connection error: {err}"),
            Self::Decode(err) => write!(f, "could not parse API response: {err}"),
            Self::Empty => write!(f, "model returned no response"),
        }
    }
}

impl std::error::Error for ClientError {}

/// A configured chat-completions client.
pub struct Client {
    agent: ureq::Agent,
    endpoint: String,
    model: String,
    api_key: String,
}

impl Client {
    /// Create a client targeting `base_url` with the given `model` and `api_key`.
    pub fn new(base_url: &str, model: &str, api_key: String) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("alacritty/", env!("CARGO_PKG_VERSION")))
            .build();

        Self {
            agent,
            endpoint: format!("{}/chat/completions", base_url.trim_end_matches('/')),
            model: model.to_owned(),
            api_key,
        }
    }

    /// Run a non-streaming chat completion, returning the assistant's reply message.
    ///
    /// `tools` is an array of OpenAI tool definitions (JSON); pass an empty slice for none.
    pub fn chat(&self, messages: &[Message], tools: &[Value]) -> Result<Message, ClientError> {
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
        });
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools.to_vec());
        }

        let response = self
            .agent
            .post(&self.endpoint)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(body);

        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(status, response)) => {
                // The response body is the provider's error payload (no key in it).
                let message = response
                    .into_string()
                    .unwrap_or_else(|_| String::from("<unreadable response body>"));
                return Err(ClientError::Http { status, message: extract_error(&message) });
            },
            Err(ureq::Error::Transport(transport)) => {
                return Err(ClientError::Transport(transport.to_string()));
            },
        };

        let parsed: ChatResponse =
            response.into_json().map_err(|err| ClientError::Decode(err.to_string()))?;

        parsed.choices.into_iter().next().map(|choice| choice.message).ok_or(ClientError::Empty)
    }
}

/// Pull a human-readable message out of a provider error body, falling back to the raw text.
fn extract_error(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value.get("error")?.get("message")?.as_str().map(String::from))
        .unwrap_or_else(|| body.trim().to_owned())
}

/// Top-level chat-completions response.
#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_request_messages() {
        let messages = vec![
            Message::system("be brief"),
            Message::user("hi"),
            Message::tool_result("call_1", "output"),
        ];
        let value = serde_json::to_value(&messages).unwrap();
        assert_eq!(value[0]["role"], "system");
        assert_eq!(value[1]["role"], "user");
        assert_eq!(value[2]["role"], "tool");
        assert_eq!(value[2]["tool_call_id"], "call_1");
        // Empty tool_calls / absent content must be omitted.
        assert!(value[1].get("tool_calls").is_none());
        assert!(value[0].get("tool_call_id").is_none());
    }

    #[test]
    fn parse_tool_call_response() {
        let body = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "type": "function",
                        "function": { "name": "run_command", "arguments": "{\"command\":\"ls\"}" }
                    }]
                }
            }]
        }"#;
        let parsed: ChatResponse = serde_json::from_str(body).unwrap();
        let message = parsed.choices.into_iter().next().unwrap().message;
        assert_eq!(message.role, Role::Assistant);
        assert_eq!(message.tool_calls.len(), 1);
        assert_eq!(message.tool_calls[0].function.name, "run_command");
    }

    #[test]
    fn extract_error_prefers_structured_message() {
        let body = r#"{"error":{"message":"invalid api key","type":"auth"}}"#;
        assert_eq!(extract_error(body), "invalid api key");
        assert_eq!(extract_error("plain text"), "plain text");
    }

    /// End-to-end HTTP round-trip against a localhost mock server. Verifies the real
    /// `ureq` request/response path (headers, JSON body, tool-call parsing).
    #[test]
    fn live_roundtrip_against_mock_server() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();

            // Read the request headers (until the blank line) and capture the auth header.
            let mut buf = Vec::new();
            let mut byte = [0u8; 1];
            while !buf.ends_with(b"\r\n\r\n") {
                if stream.read(&mut byte).unwrap() == 0 {
                    break;
                }
                buf.push(byte[0]);
            }
            let request = String::from_utf8_lossy(&buf).to_lowercase();
            assert!(request.contains("authorization: bearer secret-key"));
            assert!(request.contains("post /v1/chat/completions"));

            let body = r#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"run_command","arguments":"{\"command\":\"ls\"}"}}]}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        });

        let client =
            Client::new(&format!("http://127.0.0.1:{port}/v1"), "mock", "secret-key".into());
        let reply = client.chat(&[Message::user("list files")], &[]).unwrap();

        assert_eq!(reply.role, Role::Assistant);
        assert_eq!(reply.tool_calls.len(), 1);
        assert_eq!(reply.tool_calls[0].function.name, "run_command");

        server.join().unwrap();
    }
}
