use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;

use crate::FabricError;

const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

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

    pub fn new_with_proxy(proxy_url: Option<&str>) -> Result<Self, FabricError> {
        let client = match proxy_url {
            None => reqwest::Client::new(),
            Some(proxy_url) => {
                let proxy = reqwest::Proxy::all(proxy_url)
                    .map_err(|_| FabricError::Internal("invalid egress proxy URL".to_owned()))?;
                reqwest::Client::builder()
                    .proxy(proxy)
                    .build()
                    .map_err(|_| {
                        FabricError::Internal("build proxied egress client failed".to_owned())
                    })?
            }
        };
        Ok(Self { client })
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
            .timeout(HTTP_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|_| FabricError::Internal("egress request failed".to_owned()))?;
        let status = response.status();
        let payload = response
            .text()
            .await
            .map_err(|_| FabricError::Internal("egress body read failed".to_owned()))?;

        if !status.is_success() {
            return Err(FabricError::Internal(format!(
                "egress returned status {}",
                status.as_u16()
            )));
        }

        serde_json::from_str(&payload)
            .map_err(|_| FabricError::Internal("egress returned invalid json".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_proxy_rejects_malformed_proxy_without_echoing_it() {
        let error = match DirectProxy::new_with_proxy(Some("http://user:secret@[invalid")) {
            Ok(_) => panic!("malformed proxy must fail closed"),
            Err(error) => error,
        };
        assert!(
            matches!(error, FabricError::Internal(message) if message == "invalid egress proxy URL")
        );
    }

    #[test]
    fn external_http_requests_have_a_bounded_deadline() {
        assert_eq!(HTTP_REQUEST_TIMEOUT, Duration::from_secs(30));
    }

    #[test]
    fn upstream_status_errors_do_not_include_response_bodies() {
        let error = FabricError::Internal("egress returned status 502".to_owned());
        assert_eq!(error.to_string(), "internal: egress returned status 502");
        assert!(!error.to_string().contains("private upstream response"));
    }
}
