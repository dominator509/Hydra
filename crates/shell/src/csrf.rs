use axum::http::HeaderValue;

const CSRF_HEADER: &str = "x-hydra-csrf";
const CSRF_FORM_FIELD: &str = "_csrf_token";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsrfToken(String);

impl CsrfToken {
    pub fn generate() -> Self {
        let token: String = uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .filter(|ch| ch.is_ascii_hexdigit())
            .take(64)
            .collect();
        let token = if token.len() < 64 {
            (0..64).map(|_| '0').collect()
        } else {
            token
        };
        Self(token)
    }

    pub fn from_cookie(raw: &str) -> Option<Self> {
        if raw.len() == 64 && raw.chars().all(|ch| ch.is_ascii_hexdigit()) {
            Some(Self(raw.to_owned()))
        } else {
            None
        }
    }

    pub fn valid(&self, other: &str) -> bool {
        self.0 == other
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn hidden_field(&self) -> String {
        format!(
            r#"<input type="hidden" name="{CSRF_FORM_FIELD}" value="{}">"#,
            self.0
        )
    }
}

pub fn csrf_header(token: &CsrfToken) -> HeaderValue {
    HeaderValue::from_str(token.as_str()).expect("csrf token is always valid ASCII")
}

pub fn verify_csrf(session_token: &CsrfToken, form_token: &str) -> Result<(), CsrfError> {
    if session_token.valid(form_token) {
        Ok(())
    } else {
        Err(CsrfError)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("CSRF token mismatch")]
pub struct CsrfError;
