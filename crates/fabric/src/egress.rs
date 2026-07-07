use async_trait::async_trait;
use serde_json::Value;

use crate::FabricError;

#[async_trait]
pub trait Proxy: Send + Sync {
    async fn post_json(
        &self,
        url: &str,
        bearer_token: Option<&str>,
        body: &Value,
    ) -> Result<Value, FabricError>;
}

#[derive(Clone, Default)]
pub struct DirectProxy {
    client: reqwest::Client,
}

impl DirectProxy {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Proxy for DirectProxy {
    async fn post_json(
        &self,
        url: &str,
        bearer_token: Option<&str>,
        body: &Value,
    ) -> Result<Value, FabricError> {
        let mut request = self.client.post(url).json(body);
        if let Some(token) = bearer_token {
            request = request.bearer_auth(token);
        }

        let response = request
            .send()
            .await
            .map_err(|error| FabricError::Internal(format!("egress request failed: {error}")))?;
        let status = response.status();
        let payload = response
            .text()
            .await
            .map_err(|error| FabricError::Internal(format!("egress body read failed: {error}")))?;

        if !status.is_success() {
            return Err(FabricError::Internal(format!(
                "egress returned status {}: {}",
                status.as_u16(),
                payload
            )));
        }

        serde_json::from_str(&payload)
            .map_err(|error| FabricError::Internal(format!("egress invalid json: {error}")))
    }
}
