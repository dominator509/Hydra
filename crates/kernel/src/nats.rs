use std::env;
use std::path::{Path, PathBuf};

use async_nats::Client;

#[derive(Clone, Debug)]
pub struct NatsTransportConfig {
    pub url: String,
    pub credentials_file: Option<PathBuf>,
    pub require_auth: bool,
    pub require_tls: bool,
    pub ca_file: Option<PathBuf>,
    pub client_cert_file: Option<PathBuf>,
    pub client_key_file: Option<PathBuf>,
}

impl NatsTransportConfig {
    pub fn from_environment(url: String, secure_environment: bool) -> Result<Self, String> {
        let mut errors = Vec::new();
        validate_url(&url, &mut errors);

        let require_auth = parse_bool("NATS_REQUIRE_AUTH", secure_environment, &mut errors);
        let require_tls = parse_bool("NATS_TLS_REQUIRED", secure_environment, &mut errors);
        if secure_environment && !require_auth {
            errors.push("NATS_REQUIRE_AUTH must be true in staging or prod".to_owned());
        }
        if secure_environment && !require_tls {
            errors.push("NATS_TLS_REQUIRED must be true in staging or prod".to_owned());
        }

        let credentials_file = optional_path("NATS_CREDS_FILE");
        if require_auth && credentials_file.is_none() {
            errors.push("NATS_CREDS_FILE is required when NATS auth is required".to_owned());
        }
        if let Some(path) = credentials_file.as_ref() {
            require_file("NATS_CREDS_FILE", path, &mut errors);
        }

        let ca_file = optional_path("NATS_TLS_CA_FILE");
        if let Some(path) = ca_file.as_ref() {
            require_file("NATS_TLS_CA_FILE", path, &mut errors);
        }

        let client_cert_file = optional_path("NATS_TLS_CLIENT_CERT_FILE");
        let client_key_file = optional_path("NATS_TLS_CLIENT_KEY_FILE");
        match (client_cert_file.as_ref(), client_key_file.as_ref()) {
            (Some(cert), Some(key)) => {
                require_file("NATS_TLS_CLIENT_CERT_FILE", cert, &mut errors);
                require_file("NATS_TLS_CLIENT_KEY_FILE", key, &mut errors);
            }
            (Some(_), None) => errors.push(
                "NATS_TLS_CLIENT_KEY_FILE is required when NATS_TLS_CLIENT_CERT_FILE is set"
                    .to_owned(),
            ),
            (None, Some(_)) => errors.push(
                "NATS_TLS_CLIENT_CERT_FILE is required when NATS_TLS_CLIENT_KEY_FILE is set"
                    .to_owned(),
            ),
            (None, None) => {}
        }

        if errors.is_empty() {
            Ok(Self {
                url,
                credentials_file,
                require_auth,
                require_tls,
                ca_file,
                client_cert_file,
                client_key_file,
            })
        } else {
            Err(errors.join("\n"))
        }
    }

    pub async fn connect(&self) -> Result<Client, String> {
        if self.require_auth && self.credentials_file.is_none() {
            return Err(
                "NATS credentials are required but no credentials file is configured".to_owned(),
            );
        }

        let mut options = async_nats::ConnectOptions::new().require_tls(self.require_tls);
        if let Some(path) = self.credentials_file.as_ref() {
            options = options
                .credentials_file(path)
                .await
                .map_err(|_| "failed to load NATS credentials file".to_owned())?;
        }
        if let Some(path) = self.ca_file.as_ref() {
            options = options.add_root_certificates(path.clone());
        }
        if let (Some(cert), Some(key)) = (
            self.client_cert_file.as_ref(),
            self.client_key_file.as_ref(),
        ) {
            options = options.add_client_certificate(cert.clone(), key.clone());
        }

        options
            .connect(&self.url)
            .await
            .map_err(|error| error.to_string())
    }
}

fn optional_path(name: &str) -> Option<PathBuf> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn parse_bool(name: &str, default: bool, errors: &mut Vec<String>) -> bool {
    match env::var(name) {
        Ok(value) if value.eq_ignore_ascii_case("true") => true,
        Ok(value) if value.eq_ignore_ascii_case("false") => false,
        Ok(_) => {
            errors.push(format!("{name} must be true or false"));
            default
        }
        Err(_) => default,
    }
}

fn require_file(name: &str, path: &Path, errors: &mut Vec<String>) {
    if !path.is_file() {
        errors.push(format!("{name} must name a readable file"));
    }
}

fn validate_url(url: &str, errors: &mut Vec<String>) {
    for endpoint in url.split(',').map(str::trim) {
        let Some((scheme, authority)) = endpoint.split_once("://") else {
            errors.push("NATS_URL must contain nats:// or tls:// endpoints".to_owned());
            continue;
        };
        if !matches!(scheme, "nats" | "tls") || authority.is_empty() {
            errors.push(
                "NATS_URL must contain only non-empty nats:// or tls:// endpoints".to_owned(),
            );
        }
        if authority
            .split('/')
            .next()
            .is_some_and(|value| value.contains('@'))
        {
            errors.push("NATS_URL must not contain embedded credentials".to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_validation_rejects_embedded_credentials_without_echoing_them() {
        let mut errors = Vec::new();
        validate_url("nats://user:secret@nats:4222", &mut errors);
        assert_eq!(
            errors,
            vec!["NATS_URL must not contain embedded credentials"]
        );
        assert!(!errors.join(" ").contains("secret"));
    }
}
