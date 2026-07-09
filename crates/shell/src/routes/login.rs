use std::env;

use askama::Template;
use axum::extract::Form;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;

use crate::csrf::CsrfToken;
use crate::flash::FlashMessage;
use crate::routes;

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    title: String,
    tenant: String,
    current_page: String,
    flash: Vec<FlashMessage>,
    csrf: String,
}

#[derive(Template)]
#[template(path = "login_form.html")]
struct LoginFormTemplate {
    csrf_field: String,
    error: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginForm {
    _csrf_token: Option<String>,
    _username: Option<String>,
    _password: Option<String>,
}

#[derive(Deserialize)]
pub struct LogoutForm {
    _csrf_token: String,
}

pub async fn login_page(headers: HeaderMap) -> impl IntoResponse {
    let token = CsrfToken::generate();
    let ctx = routes::PageCtx::new("Login", "login", &headers, &token);
    let template = LoginTemplate {
        title: ctx.title,
        tenant: ctx.tenant,
        current_page: ctx.current_page,
        flash: ctx.flash,
        csrf: ctx.csrf,
    };
    let cookie = routes::csrf_cookie_header(&token);
    match template.render() {
        Ok(html) => (
            StatusCode::OK,
            [
                ("content-type", "text/html; charset=utf-8"),
                ("set-cookie", cookie.to_str().unwrap_or_default()),
            ],
            html,
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn login_action(_headers: HeaderMap, Form(_form): Form<LoginForm>) -> impl IntoResponse {
    // Only available in dev mode
    if !matches!(env::var("HYDRA_ENV").ok().as_deref(), Some("dev")) {
        let token = CsrfToken::generate();
        let template = LoginFormTemplate {
            csrf_field: token.hidden_field(),
            error: Some("Login is only available in HYDRA_ENV=dev mode".into()),
        };
        let cookie = routes::csrf_cookie_header(&token);
        return match template.render() {
            Ok(html) => (
                StatusCode::FORBIDDEN,
                [
                    ("content-type", "text/html; charset=utf-8"),
                    ("set-cookie", cookie.to_str().unwrap_or_default()),
                ],
                html,
            )
                .into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };
    }

    // Set session cookie for dev admin
    let tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001")
        .expect("hardcoded dev tenant uuid should be valid");
    let cookie = routes::set_session_cookie(tenant);

    (
        StatusCode::FOUND,
        [
            ("location", "/"),
            ("set-cookie", cookie.to_str().unwrap_or_default()),
        ],
        (),
    )
        .into_response()
}

pub async fn logout_action(headers: HeaderMap, Form(form): Form<LogoutForm>) -> impl IntoResponse {
    if routes::verify_csrf(&headers, &form._csrf_token).is_err() {
        // Still redirect to login on CSRF failure
        let cookie = routes::clear_session_cookie();
        return (
            StatusCode::FOUND,
            [
                ("location", "/login"),
                ("set-cookie", cookie.to_str().unwrap_or_default()),
            ],
            (),
        )
            .into_response();
    }

    let cookie = routes::clear_session_cookie();
    (
        StatusCode::FOUND,
        [
            ("location", "/login"),
            ("set-cookie", cookie.to_str().unwrap_or_default()),
        ],
        (),
    )
        .into_response()
}
