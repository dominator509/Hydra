use std::env;
use std::net::{AddrParseError, SocketAddr};
use std::path::Path;

use axum::http::Uri;
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
    pub database_url: String,
    pub nats_url: String,
    pub hydra_vault_key: String,
    pub hydra_base_url: String,
    pub hydra_env: HydraEnv,
    pub deepseek_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub openai_compat_base_url: Option<String>,
    pub openai_compat_model: Option<String>,
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

        let database_url = required_var("DATABASE_URL", &mut errors);
        let nats_url = required_var("NATS_URL", &mut errors);
        let hydra_vault_key = required_var("HYDRA_VAULT_KEY", &mut errors);
        let hydra_base_url = required_var("HYDRA_BASE_URL", &mut errors);
        let hydra_env = parse_env(&mut errors);
        let deepseek_api_key = optional_var("DEEPSEEK_API_KEY");
        let anthropic_api_key = optional_var("ANTHROPIC_API_KEY");
        let openai_compat_base_url = optional_var("OPENAI_COMPAT_BASE_URL");
        let openai_compat_model = optional_var("OPENAI_COMPAT_MODEL");
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
            if url.parse::<Uri>().is_err() {
                errors.push("HYDRA_BASE_URL must be a valid absolute URI".to_owned());
            }
            if matches!(hydra_env, Some(HydraEnv::Staging | HydraEnv::Prod))
                && !url.starts_with("https://")
            {
                errors.push(
                    "HYDRA_BASE_URL must use https:// when HYDRA_ENV is staging or prod".to_owned(),
                );
            }
        }

        if let Some(url) = openai_compat_base_url.as_ref() {
            if url.parse::<Uri>().is_err() {
                errors.push("OPENAI_COMPAT_BASE_URL must be a valid absolute URI".to_owned());
            }
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
            validate_absolute_uri("NEXUS_ALLOWED_MCP_ORIGINS entry", origin, &mut errors);
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
                database_url: database_url.expect("validated above"),
                nats_url: nats_url.expect("validated above"),
                hydra_vault_key: hydra_vault_key.expect("validated above"),
                hydra_base_url: hydra_base_url.expect("validated above"),
                hydra_env: hydra_env.expect("validated above"),
                deepseek_api_key,
                anthropic_api_key,
                openai_compat_base_url,
                openai_compat_model,
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
        Ok(uri) if uri.scheme().is_some() && uri.authority().is_some() => {}
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
