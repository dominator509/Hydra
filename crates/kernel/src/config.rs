use std::env;
use std::net::{AddrParseError, SocketAddr};

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
    pub tk_hit_ratio_target: f64,
    pub tk_output_budget_bytes: u32,
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
        let tk_hit_ratio_target = parse_hit_ratio(&mut errors);
        let tk_output_budget_bytes = parse_output_budget(&mut errors);

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
                tk_hit_ratio_target: tk_hit_ratio_target.expect("validated above"),
                tk_output_budget_bytes: tk_output_budget_bytes.expect("validated above"),
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
