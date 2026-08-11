use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use reqwest::header::{ACCEPT, ORIGIN};
use serde_json::{json, Map, Value};

const NEXUS_ORIGIN: &str = "https://nexus.test";

#[derive(Clone)]
pub struct FakeMcpClient {
    client: reqwest::Client,
    endpoint: String,
    token: String,
    next_id: Arc<AtomicU64>,
}

impl FakeMcpClient {
    pub fn new(base_url: &str, token: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: format!("{}/mcp", base_url.trim_end_matches('/')),
            token: token.into(),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    pub async fn initialize(&self) -> Result<Value, FakeMcpClientError> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": fabric::mcp::MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "fake-nexus-e2e", "version": "1.0.0"}
            }),
            "fake-nexus-initialize",
            None,
        )
        .await
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        correlation_id: &str,
        causation_id: Option<&str>,
    ) -> Result<Value, FakeMcpClientError> {
        self.request(
            "tools/call",
            json!({"name": name, "arguments": arguments}),
            correlation_id,
            causation_id,
        )
        .await
    }

    pub fn structured_content(result: &Value) -> Option<&Map<String, Value>> {
        result.get("structuredContent").and_then(Value::as_object)
    }

    async fn request(
        &self,
        method: &str,
        params: Value,
        correlation_id: &str,
        causation_id: Option<&str>,
    ) -> Result<Value, FakeMcpClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut request = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.token)
            .header(ORIGIN, NEXUS_ORIGIN)
            .header(ACCEPT, "application/json, text/event-stream")
            .header("mcp-protocol-version", fabric::mcp::MCP_PROTOCOL_VERSION)
            .header("x-request-id", format!("fake-nexus-{id}"))
            .header("x-correlation-id", correlation_id)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params
            }));
        if let Some(causation_id) = causation_id {
            request = request.header("x-causation-id", causation_id);
        }
        let response = request
            .send()
            .await
            .map_err(|error| FakeMcpClientError::Transport(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| FakeMcpClientError::Transport(error.to_string()))?;
        if !status.is_success() {
            return Err(FakeMcpClientError::Http {
                status: status.as_u16(),
                body,
            });
        }
        let document: Value = serde_json::from_str(&body)
            .map_err(|error| FakeMcpClientError::Protocol(error.to_string()))?;
        if let Some(error) = document.get("error") {
            return Err(FakeMcpClientError::Protocol(error.to_string()));
        }
        document
            .get("result")
            .cloned()
            .ok_or_else(|| FakeMcpClientError::Protocol("missing JSON-RPC result".to_owned()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FakeMcpClientError {
    #[error("fake MCP transport failed: {0}")]
    Transport(String),
    #[error("fake MCP HTTP request failed with status {status}: {body}")]
    Http { status: u16, body: String },
    #[error("fake MCP protocol response was invalid: {0}")]
    Protocol(String),
}
