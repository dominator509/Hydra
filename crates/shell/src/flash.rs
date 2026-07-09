use askama::Template;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, Clone)]
pub struct FlashMessage {
    pub level: FlashLevel,
    pub text: String,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashLevel {
    Success,
    Info,
    Warning,
    Error,
}

impl FlashMessage {
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Success,
            text: text.into(),
            code: None,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Error,
            text: text.into(),
            code: None,
        }
    }

    pub fn warning(text: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Warning,
            text: text.into(),
            code: None,
        }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn css_class(&self) -> &'static str {
        match self.level {
            FlashLevel::Success => "flash-success",
            FlashLevel::Info => "flash-info",
            FlashLevel::Warning => "flash-warning",
            FlashLevel::Error => "flash-error",
        }
    }
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "components/flash.html")]
pub struct FlashTemplate {
    pub flash: Vec<FlashMessage>,
}

impl IntoResponse for FlashTemplate {
    fn into_response(self) -> Response {
        match Template::render(&self) {
            Ok(html) => (
                StatusCode::OK,
                [("content-type", "text/html; charset=utf-8")],
                html,
            )
                .into_response(),
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Template error: {error}"),
            )
                .into_response(),
        }
    }
}
