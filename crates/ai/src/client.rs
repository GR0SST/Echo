use crate::{
    credentials::{CredentialStore, Secret},
    Authentication, Provider, ProviderConfig,
};
use serde_json::{json, Value};
use std::{
    fmt,
    io::BufReader,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn check(&self) -> Result<(), AiError> {
        if self.0.load(Ordering::Acquire) {
            Err(AiError::Cancelled)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiError {
    NotConfigured,
    InvalidEndpoint,
    Credentials,
    Network,
    Unauthorized,
    RateLimited,
    Provider,
    InvalidResponse,
    Incomplete,
    Cancelled,
    Identity,
}
impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotConfigured => "Choose an AI provider and model in Max settings",
            Self::InvalidEndpoint => "The AI provider URL is invalid",
            Self::Credentials => "The system credential store is unavailable",
            Self::Network => "Could not connect to your AI provider",
            Self::Unauthorized => "Reconnect your AI provider in Max settings",
            Self::RateLimited => "Your AI provider's usage limit was reached",
            Self::Provider => "Your AI provider could not complete the request",
            Self::InvalidResponse => "Your AI provider returned an invalid response",
            Self::Incomplete => "Your AI provider's response was interrupted",
            Self::Cancelled => "Cancelled",
            Self::Identity => "ChatGPT sign-in could not be verified",
        })
    }
}
impl std::error::Error for AiError {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub text: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}
impl Role {
    fn wire(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}
pub struct ChatRequest {
    pub instructions: String,
    pub messages: Vec<Message>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamEvent {
    Text(String),
    Complete,
}

fn sensitive(value: &str) -> Result<ureq::http::HeaderValue, AiError> {
    let mut value =
        ureq::http::HeaderValue::from_bytes(value.as_bytes()).map_err(|_| AiError::Credentials)?;
    value.set_sensitive(true);
    Ok(value)
}
pub(crate) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(120)))
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_body(Some(Duration::from_secs(20)))
        .build()
        .into()
}
pub(crate) fn status(status: u16) -> Result<(), AiError> {
    match status {
        200..=299 => Ok(()),
        401 | 403 => Err(AiError::Unauthorized),
        429 => Err(AiError::RateLimited),
        _ => Err(AiError::Provider),
    }
}
pub struct Client {
    pub store: Arc<dyn CredentialStore>,
}
impl Client {
    /// Blocking transport: call from a background executor. Cancellation makes
    /// late results inert; read timeouts also bound a silent provider's socket.
    pub fn chat(
        &self,
        config: &ProviderConfig,
        request: &ChatRequest,
        cancel: &Cancellation,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<(), AiError> {
        cancel.check()?;
        if config.authentication == Authentication::Disconnected {
            return Err(AiError::NotConfigured);
        }
        if config.model.trim().is_empty() || request.messages.is_empty() {
            return Err(AiError::NotConfigured);
        }
        if config.authentication == Authentication::ApiKey && config.api_key_id.is_empty() {
            return Err(AiError::NotConfigured);
        }
        let secret = if config.authentication == Authentication::ChatGpt {
            crate::oauth::access_token(self.store.as_ref(), &config.chatgpt_account_id, cancel)?
        } else {
            self.store
                .get(&config.api_key_id)
                .map_err(|_| AiError::Credentials)?
                .ok_or(AiError::NotConfigured)?
        };
        self.chat_with_secret(config, request, &secret, cancel, sink)
    }
    fn chat_with_secret(
        &self,
        config: &ProviderConfig,
        request: &ChatRequest,
        secret: &Secret,
        cancel: &Cancellation,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<(), AiError> {
        let (path, body) = request_body(config, request);
        let endpoint = config
            .endpoint(path)
            .map_err(|_| AiError::InvalidEndpoint)?;
        let agent = agent();
        let mut http = agent
            .post(endpoint.as_str())
            .header("Accept", "text/event-stream");
        http = if config.provider == Provider::Anthropic {
            http.header("x-api-key", sensitive(secret.expose())?)
                .header("anthropic-version", "2023-06-01")
        } else {
            http.header(
                "Authorization",
                sensitive(&format!("Bearer {}", secret.expose()))?,
            )
        };
        let mut response = http.send_json(body).map_err(|_| AiError::Network)?;
        status(response.status().as_u16())?;
        cancel.check()?;
        let mut reader = BufReader::new(response.body_mut().as_reader());
        consume(config.provider, &mut reader, cancel, sink)
    }
    pub fn models(
        &self,
        config: &ProviderConfig,
        cancel: &Cancellation,
    ) -> Result<Vec<Model>, AiError> {
        cancel.check()?;
        if config.authentication == Authentication::Disconnected {
            return Err(AiError::NotConfigured);
        }
        if config.authentication == Authentication::ApiKey && config.api_key_id.is_empty() {
            return Err(AiError::NotConfigured);
        }
        let secret = if config.authentication == Authentication::ChatGpt {
            crate::oauth::access_token(self.store.as_ref(), &config.chatgpt_account_id, cancel)?
        } else {
            self.store
                .get(&config.api_key_id)
                .map_err(|_| AiError::Credentials)?
                .ok_or(AiError::NotConfigured)?
        };
        let endpoint = config
            .endpoint("models")
            .map_err(|_| AiError::InvalidEndpoint)?;
        let agent = agent();
        let request = agent.get(endpoint.as_str());
        let request = if config.provider == Provider::Anthropic {
            request
                .header("x-api-key", sensitive(secret.expose())?)
                .header("anthropic-version", "2023-06-01")
        } else {
            request.header(
                "Authorization",
                sensitive(&format!("Bearer {}", secret.expose()))?,
            )
        };
        let mut response = request.call().map_err(|_| AiError::Network)?;
        status(response.status().as_u16())?;
        let value: Value = response
            .body_mut()
            .with_config()
            .limit(2 * 1024 * 1024)
            .read_json()
            .map_err(|_| AiError::InvalidResponse)?;
        cancel.check()?;
        let rows = value
            .get("models")
            .or_else(|| value.get("data"))
            .and_then(Value::as_array)
            .ok_or(AiError::InvalidResponse)?;
        Ok(rows
            .iter()
            .filter(|v| v.get("visibility").is_none_or(|s| s == "list"))
            .filter_map(|v| {
                let id = v.get("slug").or_else(|| v.get("id"))?.as_str()?.to_string();
                Some(Model {
                    name: v
                        .get("display_name")
                        .and_then(Value::as_str)
                        .unwrap_or(&id)
                        .to_string(),
                    id,
                })
            })
            .collect())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub id: String,
    pub name: String,
}
fn request_body(config: &ProviderConfig, request: &ChatRequest) -> (&'static str, Value) {
    let messages: Vec<_> = request
        .messages
        .iter()
        .map(|m| json!({"role":m.role.wire(), "content":m.text}))
        .collect();
    match config.provider {
        Provider::OpenAi => (
            "responses",
            json!({"model":config.model,"instructions":request.instructions,"input":messages,"store":false,"stream":true}),
        ),
        Provider::Anthropic => (
            "messages",
            json!({"model":config.model,"system":request.instructions,"messages":messages,"max_tokens":4096,"stream":true}),
        ),
        Provider::OpenAiCompatible => {
            let mut all = vec![json!({"role":"system","content":request.instructions})];
            all.extend(messages);
            (
                "chat/completions",
                json!({"model":config.model,"messages":all,"stream":true}),
            )
        }
    }
}
fn consume(
    provider: Provider,
    reader: &mut impl std::io::BufRead,
    cancel: &Cancellation,
    sink: &mut dyn FnMut(StreamEvent),
) -> Result<(), AiError> {
    let mut count = 0usize;
    while let Some(data) = crate::sse::read_event(reader, cancel)? {
        if provider == Provider::OpenAiCompatible && data == "[DONE]" {
            cancel.check()?;
            sink(StreamEvent::Complete);
            return Ok(());
        }
        let value: Value = serde_json::from_str(&data).map_err(|_| AiError::InvalidResponse)?;
        if value.get("error").is_some_and(|e| !e.is_null()) {
            return Err(AiError::Provider);
        }
        let (text, complete) = match provider {
            Provider::OpenAi => match value.get("type").and_then(Value::as_str) {
                Some("response.output_text.delta") => {
                    (value.get("delta").and_then(Value::as_str), false)
                }
                Some("response.completed") => (None, true),
                Some("response.failed" | "error") => return Err(AiError::Provider),
                Some("response.incomplete") => return Err(AiError::Incomplete),
                _ => (None, false),
            },
            Provider::Anthropic => match value.get("type").and_then(Value::as_str) {
                Some("content_block_delta") => {
                    (value.pointer("/delta/text").and_then(Value::as_str), false)
                }
                Some("message_delta")
                    if value
                        .pointer("/delta/stop_reason")
                        .and_then(Value::as_str)
                        .is_some_and(|s| s != "end_turn" && s != "stop_sequence") =>
                {
                    return Err(AiError::Incomplete)
                }
                Some("message_stop") => (None, true),
                Some("error") => return Err(AiError::Provider),
                _ => (None, false),
            },
            Provider::OpenAiCompatible => {
                if value
                    .pointer("/choices/0/finish_reason")
                    .and_then(Value::as_str)
                    .is_some_and(|r| r != "stop")
                {
                    return Err(AiError::Incomplete);
                }
                (
                    value
                        .pointer("/choices/0/delta/content")
                        .and_then(Value::as_str),
                    false,
                )
            }
        };
        if let Some(text) = text {
            count += text.len();
            if count > 2 * 1024 * 1024 {
                return Err(AiError::InvalidResponse);
            }
            cancel.check()?;
            sink(StreamEvent::Text(text.into()));
        }
        if complete {
            cancel.check()?;
            sink(StreamEvent::Complete);
            return Ok(());
        }
    }
    Err(AiError::Incomplete)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::testing::MemoryStore;
    use std::{
        io::{BufRead, Read, Write},
        net::TcpListener,
    };
    #[test]
    fn streaming_contracts_handle_deltas_completion_errors_and_interruption() {
        for (provider, stream) in [
            (Provider::OpenAi, "event: response.output_text.delta\r\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hé\"}\r\n\r\ndata: {\"type\":\"response.completed\"}\n\n"),
            (Provider::Anthropic, ": ping\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"hé\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n"),
            (Provider::OpenAiCompatible, "data: {\"choices\":[{\"delta\":{\"content\":\"hé\"}}]}\n\ndata: [DONE]\n\n"),
        ] {
            let mut events = Vec::new(); consume(provider, &mut std::io::Cursor::new(stream), &Cancellation::default(), &mut |e| events.push(e)).unwrap();
            assert_eq!(events, [StreamEvent::Text("hé".into()), StreamEvent::Complete]);
        }
        assert_eq!(
            consume(
                Provider::OpenAi,
                &mut std::io::Cursor::new("data: {\"type\":\"response.failed\"}\n\n"),
                &Cancellation::default(),
                &mut |_| {}
            ),
            Err(AiError::Provider)
        );
        assert_eq!(
            consume(
                Provider::OpenAi,
                &mut std::io::Cursor::new(
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n"
                ),
                &Cancellation::default(),
                &mut |_| {}
            ),
            Err(AiError::Incomplete)
        );
    }
    #[test]
    fn compatible_request_streams_directly_with_only_its_stored_key_and_cancels() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let receiver = std::thread::spawn(move || {
            let (mut socket, _) = server.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut headers = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                headers.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1"));
            assert!(headers
                .to_ascii_lowercase()
                .contains("authorization: bearer local-test-key"));
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|n| n.trim().parse().unwrap())
                })
                .unwrap();
            let mut bytes = vec![0; length];
            reader.read_exact(&mut bytes).unwrap();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body["model"], "local-model");
            assert_eq!(body["stream"], true);
            let body =
                "data: {\"choices\":[{\"delta\":{\"content\":\"first\"}}]}\n\ndata: [DONE]\n\n";
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        });
        let store = Arc::new(MemoryStore::default());
        store
            .set("local", &Secret::new("local-test-key".into()))
            .unwrap();
        let config = ProviderConfig {
            provider: Provider::OpenAiCompatible,
            model: "local-model".into(),
            base_url: format!("http://{address}/v1"),
            api_key_id: "local".into(),
            ..Default::default()
        };
        let client = Client { store };
        let cancel = Cancellation::default();
        let mut events = Vec::new();
        assert_eq!(
            client.chat(
                &config,
                &ChatRequest {
                    instructions: "Answer".into(),
                    messages: vec![Message {
                        role: Role::User,
                        text: "hello".into()
                    }]
                },
                &cancel,
                &mut |e| {
                    events.push(e);
                    cancel.cancel();
                }
            ),
            Err(AiError::Cancelled)
        );
        assert_eq!(events, [StreamEvent::Text("first".into())]);
        receiver.join().unwrap();
    }
    #[test]
    fn signing_out_never_falls_back_to_a_saved_api_key() {
        use crate::credentials::CredentialError;
        struct NoCredentials;
        impl CredentialStore for NoCredentials {
            fn get(&self, _: &str) -> Result<Option<Secret>, CredentialError> {
                panic!("Disconnected requests cannot read any credential")
            }
            fn set(&self, _: &str, _: &Secret) -> Result<(), CredentialError> {
                unreachable!()
            }
            fn delete(&self, _: &str) -> Result<(), CredentialError> {
                unreachable!()
            }
        }
        let client = Client {
            store: Arc::new(NoCredentials),
        };
        let config = ProviderConfig {
            authentication: Authentication::Disconnected,
            model: "selected-model".into(),
            api_key_id: "saved-api-key-reference".into(),
            ..Default::default()
        };
        let request = ChatRequest {
            instructions: String::new(),
            messages: vec![Message {
                role: Role::User,
                text: "hello".into(),
            }],
        };
        assert_eq!(
            client.chat(&config, &request, &Cancellation::default(), &mut |_| {}),
            Err(AiError::NotConfigured)
        );
        assert_eq!(
            client.models(&config, &Cancellation::default()),
            Err(AiError::NotConfigured)
        );
    }
    #[test]
    fn chatgpt_request_uses_public_stateless_streaming_contract_without_unsupported_fields() {
        let config = ProviderConfig {
            authentication: Authentication::ChatGpt,
            ..Default::default()
        };
        let (path, body) = request_body(
            &config,
            &ChatRequest {
                instructions: "Be brief".into(),
                messages: vec![Message {
                    role: Role::User,
                    text: "hi".into(),
                }],
            },
        );
        assert_eq!(path, "responses");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert!(body["input"].is_array());
        assert!(body.get("max_output_tokens").is_none());
        assert!(body.get("previous_response_id").is_none());
    }
}
