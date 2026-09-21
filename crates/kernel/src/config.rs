use std::env;
use std::net::{AddrParseError, SocketAddr};
use std::path::Path;

use axum::http::Uri;
use bridge_host::DEFAULT_VAULT_PATH;
use hydra_kernel::nats::NatsTransportConfig;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HydraEnv {
    Dev,
    Staging,
    Prod,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: SocketAddr,
    pub shutdown_timeout_seconds: u64,
    pub dependency_timeout_seconds: u64,
    pub database_url: String,
    pub nats_transport: NatsTransportConfig,
    pub hydra_vault_key: String,
    pub hydra_vault_path: String,
    pub hydra_base_url: String,
    pub hydra_env: HydraEnv,
    pub egress_proxy_url: Option<String>,
    pub deepseek_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub openai_compat_base_url: Option<String>,
    pub openai_compat_model: Option<String>,
    pub nexus_model_gateway_url: Option<String>,
    pub nexus_model_gateway_model: Option<String>,
    pub nexus_model_gateway_token_secret: Option<String>,
    pub nexus_model_gateway_private: bool,
    pub hydra_skills_path: Option<String>,
    pub hydra_skills_trust_file: Option<String>,
    pub hydra_adapters_path: Option<String>,
    pub hydra_bridge_sync_scheduler_enabled: bool,
    pub tk_hit_ratio_target: f64,
    pub tk_output_budget_bytes: u32,
    pub governor_monthly_spend_cap_cents: u64,
    pub governor_pii_egress_allowlist: Vec<String>,
    pub governor_blast_entities_ceiling: u32,
    pub governor_blast_sends_ceiling: u32,
    pub governor_blast_money_ceiling_cents: u64,
    pub nexus_integration_enabled: bool,
    pub nexus_oidc_issuer: Option<String>,
    pub nexus_oidc_audience: Option<String>,
    pub nexus_oidc_jwks_url: Option<String>,
    pub nexus_oidc_public_key_file: Option<String>,
    pub nexus_oidc_allowed_algorithms: Vec<String>,
    pub nexus_allowed_mcp_origins: Vec<String>,
    pub nexus_jwks_cache_seconds: u64,
    pub nexus_oidc_clock_skew_seconds: u64,
    pub nexus_mcp_max_request_bytes: usize,
    pub nexus_approval_auth_strengths: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config validation failed:\n{0}")]
    Invalid(String),
    #[error("invalid HYDRA_BIND value '{raw}': {source}")]
    InvalidBind { raw: String, source: AddrParseError },
}

impl Config {
    pub fn validate() -> Result<Self, ConfigError> {
        let mut errors = Vec::new();

        let bind_raw = env::var("HYDRA_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
        let bind = bind_raw
            .parse()
            .map_err(|source| ConfigError::InvalidBind {
                raw: bind_raw,
                source,
            })?;
        let shutdown_timeout_seconds =
            parse_positive_u64("HYDRA_SHUTDOWN_TIMEOUT_SECONDS", 30, &mut errors);
        let dependency_timeout_seconds =
            parse_positive_u64("HYDRA_DEPENDENCY_TIMEOUT_SECONDS", 5, &mut errors);

        let database_url = required_var("DATABASE_URL", &mut errors);
        let nats_url = required_var("NATS_URL", &mut errors);
        let hydra_vault_key = required_var("HYDRA_VAULT_KEY", &mut errors);
        let hydra_vault_path =
            optional_var("HYDRA_VAULT_PATH").unwrap_or_else(|| DEFAULT_VAULT_PATH.to_owned());
        let hydra_base_url = required_var("HYDRA_BASE_URL", &mut errors);
        let hydra_env = parse_env(&mut errors);
        let nats_transport = nats_url.as_ref().and_then(|url| {
            NatsTransportConfig::from_environment(
                url.clone(),
                matches!(hydra_env, Some(HydraEnv::Staging | HydraEnv::Prod)),
            )
            .map_err(|error| errors.push(error))
            .ok()
        });
        let egress_proxy_url = optional_var("HYDRA_EGRESS_PROXY_URL");
        validate_egress_proxy(egress_proxy_url.as_ref(), hydra_env, &mut errors);
        let deepseek_api_key = optional_var("DEEPSEEK_API_KEY");
        let anthropic_api_key = optional_var("ANTHROPIC_API_KEY");
        let openai_compat_base_url = optional_var("OPENAI_COMPAT_BASE_URL");
        let openai_compat_model = optional_var("OPENAI_COMPAT_MODEL");
        let nexus_model_gateway_url = optional_var("NEXUS_MODEL_GATEWAY_URL");
        let nexus_model_gateway_model = optional_var("NEXUS_MODEL_GATEWAY_MODEL");
        let nexus_model_gateway_token_secret = optional_var("NEXUS_MODEL_GATEWAY_TOKEN_SECRET");
        let nexus_model_gateway_private =
            parse_bool("NEXUS_MODEL_GATEWAY_PRIVATE", false, &mut errors);
        let hydra_skills_path = optional_var("HYDRA_SKILLS_PATH");
        let hydra_skills_trust_file = optional_var("HYDRA_SKILLS_TRUST_FILE");
        let hydra_adapters_path = optional_var("HYDRA_ADAPTERS_PATH");
        let hydra_bridge_sync_scheduler_enabled =
            parse_bool("HYDRA_BRIDGE_SYNC_SCHEDULER_ENABLED", false, &mut errors);
        let tk_hit_ratio_target = parse_hit_ratio(&mut errors);
        let tk_output_budget_bytes = parse_output_budget(&mut errors);
        let governor_monthly_spend_cap_cents = parse_positive_u64(
            "HYDRA_GOVERNOR_MONTHLY_SPEND_CAP_CENTS",
            50_000,
            &mut errors,
        );
        let governor_pii_egress_allowlist =
            comma_list_with_default("HYDRA_GOVERNOR_PII_EGRESS_ALLOWLIST", "private");
        let governor_blast_entities_ceiling =
            parse_u32("HYDRA_GOVERNOR_BLAST_ENTITIES_CEILING", 250, &mut errors);
        let governor_blast_sends_ceiling =
            parse_u32("HYDRA_GOVERNOR_BLAST_SENDS_CEILING", 50, &mut errors);
        let governor_blast_money_ceiling_cents = parse_u64(
            "HYDRA_GOVERNOR_BLAST_MONEY_CEILING_CENTS",
            250_000,
            &mut errors,
        );
        let nexus_integration_enabled = parse_bool("NEXUS_INTEGRATION_ENABLED", false, &mut errors);
        let nexus_oidc_issuer = optional_var("NEXUS_OIDC_ISSUER");
        let nexus_oidc_audience = optional_var("NEXUS_OIDC_AUDIENCE");
        let nexus_oidc_jwks_url = optional_var("NEXUS_OIDC_JWKS_URL");
        let nexus_oidc_public_key_file = optional_var("NEXUS_OIDC_PUBLIC_KEY_FILE");
        let nexus_oidc_allowed_algorithms =
            comma_list_with_default("NEXUS_OIDC_ALLOWED_ALGORITHMS", "RS256");
        let nexus_allowed_mcp_origins = comma_list("NEXUS_ALLOWED_MCP_ORIGINS");
        let nexus_jwks_cache_seconds =
            parse_positive_u64("NEXUS_JWKS_CACHE_SECONDS", 300, &mut errors);
        let nexus_oidc_clock_skew_seconds =
            parse_u64("NEXUS_OIDC_CLOCK_SKEW_SECONDS", 30, &mut errors);
        let nexus_mcp_max_request_bytes =
            parse_positive_usize("NEXUS_MCP_MAX_REQUEST_BYTES", 1_048_576, &mut errors);
        let nexus_approval_auth_strengths =
            comma_list_with_default("NEXUS_APPROVAL_AUTH_STRENGTHS", "mfa");

        if let Some(url) = hydra_base_url.as_ref() {
            validate_absolute_uri("HYDRA_BASE_URL", url, &mut errors);
            if matches!(hydra_env, Some(HydraEnv::Staging | HydraEnv::Prod))
                && !url.starts_with("https://")
            {
                errors.push(
                    "HYDRA_BASE_URL must use https:// when HYDRA_ENV is staging or prod".to_owned(),
                );
            }
        }

        if let Some(url) = openai_compat_base_url.as_ref() {
            validate_absolute_uri("OPENAI_COMPAT_BASE_URL", url, &mut errors);
            if openai_compat_model.is_none() {
                errors.push(
                    "OPENAI_COMPAT_MODEL is required when OPENAI_COMPAT_BASE_URL is set".to_owned(),
                );
            }
        } else if openai_compat_model.is_some() {
            errors.push(
                "OPENAI_COMPAT_BASE_URL is required when OPENAI_COMPAT_MODEL is set".to_owned(),
            );
        }

        match (
            nexus_model_gateway_url.as_ref(),
            nexus_model_gateway_model.as_ref(),
            nexus_model_gateway_token_secret.as_ref(),
        ) {
            (Some(url), Some(_), Some(_)) => {
                validate_nexus_uri("NEXUS_MODEL_GATEWAY_URL", Some(url), hydra_env, &mut errors);
            }
            (Some(_), None, _) => errors.push(
                "NEXUS_MODEL_GATEWAY_MODEL is required when NEXUS_MODEL_GATEWAY_URL is set"
                    .to_owned(),
            ),
            (Some(_), _, None) => errors.push(
                "NEXUS_MODEL_GATEWAY_TOKEN_SECRET is required when NEXUS_MODEL_GATEWAY_URL is set"
                    .to_owned(),
            ),
            (None, Some(_), _) => errors.push(
                "NEXUS_MODEL_GATEWAY_URL is required when NEXUS_MODEL_GATEWAY_MODEL is set"
                    .to_owned(),
            ),
            (None, _, Some(_)) => errors.push(
                "NEXUS_MODEL_GATEWAY_URL is required when NEXUS_MODEL_GATEWAY_TOKEN_SECRET is set"
                    .to_owned(),
            ),
            (None, None, None) => {}
        }

        match (hydra_skills_path.as_ref(), hydra_skills_trust_file.as_ref()) {
            (Some(path), Some(trust_file)) => {
                if !Path::new(path).is_dir() {
                    errors.push(format!(
                        "HYDRA_SKILLS_PATH does not name a readable directory: {path}"
                    ));
                }
                if !Path::new(trust_file).is_file() {
                    errors.push(format!(
                        "HYDRA_SKILLS_TRUST_FILE does not name a readable file: {trust_file}"
                    ));
                }
            }
            (Some(_), None) => errors.push(
                "HYDRA_SKILLS_TRUST_FILE is required when HYDRA_SKILLS_PATH is set".to_owned(),
            ),
            (None, Some(_)) => errors.push(
                "HYDRA_SKILLS_PATH is required when HYDRA_SKILLS_TRUST_FILE is set".to_owned(),
            ),
            (None, None) => {}
        }

        if nexus_integration_enabled {
            require_present("NEXUS_OIDC_ISSUER", nexus_oidc_issuer.as_ref(), &mut errors);
            require_present(
                "NEXUS_OIDC_AUDIENCE",
                nexus_oidc_audience.as_ref(),
                &mut errors,
            );
            if nexus_oidc_jwks_url.is_none() && nexus_oidc_public_key_file.is_none() {
                errors.push(
                    "Nexus integration requires NEXUS_OIDC_JWKS_URL or NEXUS_OIDC_PUBLIC_KEY_FILE"
                        .to_owned(),
                );
            }
            if nexus_oidc_jwks_url.is_some() && nexus_oidc_public_key_file.is_some() {
                errors.push(
                    "configure only one of NEXUS_OIDC_JWKS_URL or NEXUS_OIDC_PUBLIC_KEY_FILE"
                        .to_owned(),
                );
            }
            if nexus_allowed_mcp_origins.is_empty() {
                errors.push(
                    "NEXUS_ALLOWED_MCP_ORIGINS must contain at least one allowed Origin when Nexus integration is enabled"
                        .to_owned(),
                );
            }
        }
        if nexus_oidc_allowed_algorithms.is_empty() {
            errors.push("NEXUS_OIDC_ALLOWED_ALGORITHMS cannot be empty".to_owned());
        }
        for algorithm in &nexus_oidc_allowed_algorithms {
            if fabric::parse_algorithm(algorithm).is_err() {
                errors.push(format!(
                    "NEXUS_OIDC_ALLOWED_ALGORITHMS contains unsupported asymmetric algorithm '{algorithm}'"
                ));
            }
        }

        validate_nexus_uri(
            "NEXUS_OIDC_ISSUER",
            nexus_oidc_issuer.as_ref(),
            hydra_env,
            &mut errors,
        );
        validate_nexus_uri(
            "NEXUS_OIDC_JWKS_URL",
            nexus_oidc_jwks_url.as_ref(),
            hydra_env,
            &mut errors,
        );
        for origin in &nexus_allowed_mcp_origins {
            validate_nexus_uri(
                "NEXUS_ALLOWED_MCP_ORIGINS entry",
                Some(origin),
                hydra_env,
                &mut errors,
            );
        }
        if nexus_integration_enabled {
            if let Some(path) = nexus_oidc_public_key_file.as_ref() {
                if !Path::new(path).is_file() {
                    errors.push(format!(
                        "NEXUS_OIDC_PUBLIC_KEY_FILE does not name a readable file: {path}"
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(Self {
                bind,
                shutdown_timeout_seconds: shutdown_timeout_seconds.expect("validated above"),
                dependency_timeout_seconds: dependency_timeout_seconds.expect("validated above"),
                database_url: database_url.expect("validated above"),
                nats_transport: nats_transport.expect("validated above"),
                hydra_vault_key: hydra_vault_key.expect("validated above"),
                hydra_vault_path,
                hydra_base_url: hydra_base_url.expect("validated above"),
                hydra_env: hydra_env.expect("validated above"),
                egress_proxy_url,
                deepseek_api_key,
                anthropic_api_key,
                openai_compat_base_url,
                openai_compat_model,
                nexus_model_gateway_url,
                nexus_model_gateway_model,
                nexus_model_gateway_token_secret,
                nexus_model_gateway_private,
                hydra_skills_path,
                hydra_skills_trust_file,
                hydra_adapters_path,
                hydra_bridge_sync_scheduler_enabled,
                tk_hit_ratio_target: tk_hit_ratio_target.expect("validated above"),
                tk_output_budget_bytes: tk_output_budget_bytes.expect("validated above"),
                governor_monthly_spend_cap_cents: governor_monthly_spend_cap_cents
                    .expect("validated above"),
                governor_pii_egress_allowlist,
                governor_blast_entities_ceiling: governor_blast_entities_ceiling
                    .expect("validated above"),
                governor_blast_sends_ceiling: governor_blast_sends_ceiling
                    .expect("validated above"),
                governor_blast_money_ceiling_cents: governor_blast_money_ceiling_cents
                    .expect("validated above"),
                nexus_integration_enabled,
                nexus_oidc_issuer,
                nexus_oidc_audience,
                nexus_oidc_jwks_url,
                nexus_oidc_public_key_file,
                nexus_oidc_allowed_algorithms,
                nexus_allowed_mcp_origins,
                nexus_jwks_cache_seconds: nexus_jwks_cache_seconds.expect("validated above"),
                nexus_oidc_clock_skew_seconds: nexus_oidc_clock_skew_seconds
                    .expect("validated above"),
                nexus_mcp_max_request_bytes: nexus_mcp_max_request_bytes.expect("validated above"),
                nexus_approval_auth_strengths,
            })
        } else {
            Err(ConfigError::Invalid(errors.join("\n")))
        }
    }
}

fn required_var(name: &str, errors: &mut Vec<String>) -> Option<String> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => {
            errors.push(format!("{name} is required"));
            None
        }
    }
}

fn optional_var(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn comma_list(name: &str) -> Vec<String> {
    optional_var(name)
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn comma_list_with_default(name: &str, default: &str) -> Vec<String> {
    optional_var(name)
        .unwrap_or_else(|| default.to_owned())
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_bool(name: &str, default: bool, errors: &mut Vec<String>) -> bool {
    match env::var(name) {
        Ok(value) if value.eq_ignore_ascii_case("true") => true,
        Ok(value) if value.eq_ignore_ascii_case("false") => false,
        Ok(value) => {
            errors.push(format!("{name} must be true or false, got '{value}'"));
            default
        }
        Err(_) => default,
    }
}

fn parse_u64(name: &str, default: u64, errors: &mut Vec<String>) -> Option<u64> {
    let raw = env::var(name).unwrap_or_else(|_| default.to_string());
    match raw.parse::<u64>() {
        Ok(value) => Some(value),
        Err(_) => {
            errors.push(format!("{name} must be a valid u64"));
            None
        }
    }
}

fn parse_positive_u64(name: &str, default: u64, errors: &mut Vec<String>) -> Option<u64> {
    match parse_u64(name, default, errors) {
        Some(0) => {
            errors.push(format!("{name} must be greater than zero"));
            None
        }
        value => value,
    }
}

fn parse_positive_usize(name: &str, default: usize, errors: &mut Vec<String>) -> Option<usize> {
    let raw = env::var(name).unwrap_or_else(|_| default.to_string());
    match raw.parse::<usize>() {
        Ok(value) if value > 0 => Some(value),
        _ => {
            errors.push(format!("{name} must be a positive integer"));
            None
        }
    }
}

fn parse_u32(name: &str, default: u32, errors: &mut Vec<String>) -> Option<u32> {
    let raw = env::var(name).unwrap_or_else(|_| default.to_string());
    match raw.parse::<u32>() {
        Ok(value) => Some(value),
        Err(_) => {
            errors.push(format!("{name} must be a u32"));
            None
        }
    }
}

fn require_present(name: &str, value: Option<&String>, errors: &mut Vec<String>) {
    if value.is_none() {
        errors.push(format!(
            "{name} is required when Nexus integration is enabled"
        ));
    }
}

fn validate_absolute_uri(name: &str, value: &str, errors: &mut Vec<String>) {
    match value.parse::<Uri>() {
        Ok(uri) if uri.scheme().is_some() && uri.authority().is_some() => {
            if !matches!(uri.scheme_str(), Some("http" | "https")) {
                errors.push(format!("{name} must use the http:// or https:// scheme"));
            }
            if uri
                .authority()
                .is_some_and(|authority| authority.as_str().contains('@'))
            {
                errors.push(format!("{name} must not contain embedded credentials"));
            }
        }
        _ => errors.push(format!("{name} must be an absolute URI")),
    }
}

fn validate_nexus_uri(
    name: &str,
    value: Option<&String>,
    hydra_env: Option<HydraEnv>,
    errors: &mut Vec<String>,
) {
    let Some(value) = value else {
        return;
    };
    validate_absolute_uri(name, value, errors);
    if matches!(hydra_env, Some(HydraEnv::Staging | HydraEnv::Prod))
        && !value.starts_with("https://")
    {
        errors.push(format!("{name} must use https:// in staging or prod"));
    }
}

fn validate_egress_proxy(
    value: Option<&String>,
    hydra_env: Option<HydraEnv>,
    errors: &mut Vec<String>,
) {
    let required = matches!(hydra_env, Some(HydraEnv::Staging | HydraEnv::Prod));
    let Some(value) = value else {
        if required {
            errors.push("HYDRA_EGRESS_PROXY_URL is required in staging or prod".to_owned());
        }
        return;
    };

    let valid = value
        .parse::<Uri>()
        .ok()
        .map(|uri| {
            matches!(uri.scheme_str(), Some("http" | "https"))
                && uri.authority().is_some()
                && !uri
                    .authority()
                    .is_some_and(|authority| authority.as_str().contains('@'))
                && uri.path_and_query().is_none_or(|path| path.as_str() == "/")
        })
        .unwrap_or(false);

    if !valid {
        errors.push(
            "HYDRA_EGRESS_PROXY_URL must be an absolute http(s) proxy URI without embedded credentials"
                .to_owned(),
        );
    }
}

fn parse_env(errors: &mut Vec<String>) -> Option<HydraEnv> {
    match env::var("HYDRA_ENV") {
        Ok(value) => match value.trim() {
            "dev" => Some(HydraEnv::Dev),
            "staging" => Some(HydraEnv::Staging),
            "prod" => Some(HydraEnv::Prod),
            other => {
                errors.push(format!(
                    "HYDRA_ENV must be one of dev|staging|prod, got '{other}'"
                ));
                None
            }
        },
        Err(_) => {
            errors.push("HYDRA_ENV is required".to_owned());
            None
        }
    }
}

fn parse_hit_ratio(errors: &mut Vec<String>) -> Option<f64> {
    let raw = env::var("TK_HIT_RATIO_TARGET").unwrap_or_else(|_| "0.97".to_owned());
    match raw.parse::<f64>() {
        Ok(value) if value > 0.0 && value < 1.0 => Some(value),
        _ => {
            errors.push("TK_HIT_RATIO_TARGET must be a float between 0 and 1".to_owned());
            None
        }
    }
}

fn parse_output_budget(errors: &mut Vec<String>) -> Option<u32> {
    let raw = env::var("TK_OUTPUT_BUDGET_BYTES").unwrap_or_else(|_| "16384".to_owned());
    match raw.parse::<u32>() {
        Ok(value) => Some(value),
        Err(_) => {
            errors.push("TK_OUTPUT_BUDGET_BYTES must be a valid u32".to_owned());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(value: Option<&str>, env: Option<HydraEnv>) -> Vec<String> {
        let value = value.map(str::to_owned);
        let mut errors = Vec::new();
        validate_egress_proxy(value.as_ref(), env, &mut errors);
        errors
    }

    #[test]
    fn dev_may_omit_egress_proxy() {
        assert!(check(None, Some(HydraEnv::Dev)).is_empty());
    }

    #[test]
    fn staging_and_prod_require_egress_proxy() {
        assert_eq!(
            check(None, Some(HydraEnv::Staging)),
            vec!["HYDRA_EGRESS_PROXY_URL is required in staging or prod"]
        );
        assert_eq!(
            check(None, Some(HydraEnv::Prod)),
            vec!["HYDRA_EGRESS_PROXY_URL is required in staging or prod"]
        );
    }

    #[test]
    fn proxy_uri_rejects_credentials_and_non_http_schemes() {
        assert!(!check(Some("http://user:secret@proxy:8888"), Some(HydraEnv::Prod)).is_empty());
        assert!(!check(Some("ftp://proxy:21"), Some(HydraEnv::Prod)).is_empty());
        assert!(check(Some("http://egress-proxy:8888"), Some(HydraEnv::Prod)).is_empty());
        assert!(check(Some("https://proxy.example:8443/"), Some(HydraEnv::Staging)).is_empty());
    }

    #[test]
    fn proxy_validation_does_not_echo_the_value() {
        let errors = check(
            Some("http://user:super-secret@proxy.example:8888"),
            Some(HydraEnv::Prod),
        );
        assert!(!errors.join(" ").contains("super-secret"));
    }

    #[test]
    fn endpoint_uri_validation_rejects_relative_and_credential_bearing_urls() {
        let mut errors = Vec::new();
        validate_absolute_uri("TEST_ENDPOINT", "/relative", &mut errors);
        assert_eq!(errors, vec!["TEST_ENDPOINT must be an absolute URI"]);

        errors.clear();
        validate_absolute_uri(
            "TEST_ENDPOINT",
            "https://user:super-secret@example.invalid/v1",
            &mut errors,
        );
        assert_eq!(
            errors,
            vec!["TEST_ENDPOINT must not contain embedded credentials"]
        );
        assert!(!errors.join(" ").contains("super-secret"));

        errors.clear();
        validate_absolute_uri("TEST_ENDPOINT", "https://provider.example/v1", &mut errors);
        assert!(errors.is_empty());

        errors.clear();
        validate_absolute_uri("TEST_ENDPOINT", "file://local/path", &mut errors);
        assert_eq!(
            errors,
            vec!["TEST_ENDPOINT must use the http:// or https:// scheme"]
        );

        errors.clear();
        let origin = "http://nexus.example".to_owned();
        validate_nexus_uri(
            "TEST_ORIGIN",
            Some(&origin),
            Some(HydraEnv::Prod),
            &mut errors,
        );
        assert_eq!(
            errors,
            vec!["TEST_ORIGIN must use https:// in staging or prod"]
        );
    }

    #[test]
    fn positive_timeout_parser_rejects_zero_and_invalid_values() {
        let mut errors = Vec::new();
        assert_eq!(parse_positive_u64("TEST_TIMEOUT", 5, &mut errors), Some(5));

        std::env::set_var("TEST_TIMEOUT", "0");
        assert_eq!(parse_positive_u64("TEST_TIMEOUT", 5, &mut errors), None);
        std::env::set_var("TEST_TIMEOUT", "not-a-number");
        assert_eq!(parse_positive_u64("TEST_TIMEOUT", 5, &mut errors), None);
        std::env::remove_var("TEST_TIMEOUT");

        assert_eq!(errors.len(), 2);
        assert!(errors.iter().all(|error| error.contains("TEST_TIMEOUT")));
    }
}
