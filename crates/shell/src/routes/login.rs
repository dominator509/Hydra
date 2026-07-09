use askama::Template;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::Deserialize;

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
    #[serde(rename = "_csrf_token")]
    pub csrf_token: Option<String>,
    #[serde(rename = "_username")]
    pub username: Option<String>,
    #[serde(rename = "_password")]
    pub password: Option<String>,
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

pub async fn login_action(
    headers: HeaderMap,
    State(state): State<fabric::AppState>,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    // Verify CSRF by checking the form token against the hydra-csrf cookie.
    if let Err(_msg) = routes::verify_csrf_cookie(&headers, &form.csrf_token.unwrap_or_default())
    {
        let token = CsrfToken::generate();
        let template = LoginFormTemplate {
            csrf_field: token.hidden_field(),
            error: Some("CSRF token mismatch".into()),
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

    let username = form.username.unwrap_or_default();
    let password = form.password.unwrap_or_default();

    match state.auth.authenticate(&username, &password).await {
        Ok(session) => {
            let cookie = routes::set_session_cookie(&session.token);
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
        Err(_) => {
            let token = CsrfToken::generate();
            let template = LoginFormTemplate {
                csrf_field: token.hidden_field(),
                error: Some("Invalid credentials".into()),
            };
            let cookie = routes::csrf_cookie_header(&token);
            match template.render() {
                Ok(html) => (
                    StatusCode::UNAUTHORIZED,
                    [
                        ("content-type", "text/html; charset=utf-8"),
                        ("set-cookie", cookie.to_str().unwrap_or_default()),
                    ],
                    html,
                )
                    .into_response(),
                Err(e) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                }
            }
        }
    }
}

pub async fn logout_action(
    headers: HeaderMap,
    State(state): State<fabric::AppState>,
    Form(form): Form<LogoutForm>,
) -> impl IntoResponse {
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

    // Revoke the session if present.
    if let Some(token) = routes::session_token(&headers) {
        let _ = state.auth.revoke(&token).await;
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
