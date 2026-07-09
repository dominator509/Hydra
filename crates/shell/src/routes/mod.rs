pub mod agents;
pub mod approvals;
pub mod autonomy;
pub mod bridges;
pub mod login;
pub mod pipelines;
pub mod workspace;

use axum::http::{HeaderMap, HeaderValue};
use axum::routing::{get, post};
use axum::Router;
use uuid::Uuid;

use crate::csrf::CsrfToken;
use crate::flash::FlashMessage;

pub fn router(state: fabric::AppState) -> Router {
    Router::new()
        // Login / Logout
        .route("/login", get(login::login_page).post(login::login_action))
        .route("/logout", post(login::logout_action))
        // Workspace
        .route("/", get(workspace::workspace_home))
        .route("/workspace", get(workspace::workspace_home))
        // Pipelines
        .route("/pipelines", get(pipelines::pipeline_board))
        .route("/pipelines/new", post(pipelines::pipeline_new_deal))
        .route("/pipelines/:id", get(pipelines::pipeline_record))
        .route("/pipelines/:id/action", post(pipelines::pipeline_action))
        .route("/pipelines/:id/move", post(pipelines::pipeline_move))
        // Approvals
        .route("/approvals", get(approvals::approvals_list))
        .route("/approvals/:id/approve", post(approvals::approve_envelope))
        .route("/approvals/:id/reject", post(approvals::reject_envelope))
        .route("/approvals/batch-approve", post(approvals::batch_approve))
        .route("/approvals/batch-reject", post(approvals::batch_reject))
        .route("/approvals/count", get(approvals::approvals_count))
        // Autonomy
        .route("/autonomy", get(autonomy::autonomy_page))
        .route("/autonomy/save", post(autonomy::save_matrix))
        .route("/autonomy/save-kinds", post(autonomy::save_kinds))
        // Bridges
        .route("/bridges", get(bridges::bridges_list))
        .route("/bridges/register", post(bridges::register_bridge))
        .route("/bridges/:id/pause", post(bridges::pause_bridge))
        .route("/bridges/:id/resume", post(bridges::resume_bridge))
        // Agents
        .route("/agents", get(agents::agents_console))
        .with_state(state)
}

const SESSION_COOKIE: &str = "hydra-session";

pub fn session_tenant(headers: &HeaderMap) -> Option<Uuid> {
    let cookie = headers.get("cookie")?.to_str().ok()?;
    for pair in cookie.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix("hydra-session=") {
            return Uuid::parse_str(value).ok();
        }
    }
    None
}

pub fn session_cookie_value(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get("cookie")?.to_str().ok()?;
    for pair in cookie.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix("hydra-session=") {
            return Some(value.to_owned());
        }
    }
    None
}

/// Extract the raw session token from the `hydra-session` cookie.
pub fn session_token(headers: &HeaderMap) -> Option<String> {
    session_cookie_value(headers)
}

pub fn set_session_cookie(token: &str) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax",
        SESSION_COOKIE, token
    ))
    .expect("session cookie value should be valid ASCII")
}

pub fn clear_session_cookie() -> HeaderValue {
    HeaderValue::from_str(&format!("{}=; Path=/; Max-Age=0; HttpOnly", SESSION_COOKIE))
        .expect("clear cookie value should be valid ASCII")
}

pub fn csrf_cookie_header(token: &CsrfToken) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "hydra-csrf={}; Path=/; HttpOnly; SameSite=Lax",
        token.as_str()
    ))
    .expect("csrf cookie header should be valid ASCII")
}

pub fn verify_csrf(headers: &HeaderMap, form_token: &str) -> Result<(), FlashMessage> {
    let session = session_cookie_value(headers)
        .ok_or_else(|| FlashMessage::error("no session cookie for CSRF check"))?;
    if session == form_token {
        Ok(())
    } else {
        Err(FlashMessage::error("CSRF token mismatch"))
    }
}

/// Verify a CSRF token from a login form against the `hydra-csrf` cookie.
///
/// This is used during login when no session cookie exists yet.
pub fn verify_csrf_cookie(headers: &HeaderMap, form_token: &str) -> Result<(), FlashMessage> {
    let cookie = headers
        .get("cookie")
        .ok_or_else(|| FlashMessage::error("no cookie header for CSRF check"))?
        .to_str()
        .map_err(|_| FlashMessage::error("invalid cookie encoding"))?;

    let csrf_value = cookie
        .split(';')
        .map(|s| s.trim())
        .find_map(|pair| pair.strip_prefix("hydra-csrf="))
        .ok_or_else(|| FlashMessage::error("no CSRF cookie found"))?;

    let token =
        CsrfToken::from_cookie(csrf_value).ok_or_else(|| FlashMessage::error("invalid CSRF cookie value"))?;

    if token.valid(form_token) {
        Ok(())
    } else {
        Err(FlashMessage::error("CSRF token mismatch"))
    }
}

pub fn tenant_or_default(headers: &HeaderMap) -> Uuid {
    session_tenant(headers).unwrap_or_else(|| {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001")
            .expect("hardcoded dev tenant uuid should be valid")
    })
}

pub struct PageCtx {
    pub title: String,
    pub tenant: String,
    pub current_page: String,
    pub flash: Vec<FlashMessage>,
    pub csrf: String,
}

impl PageCtx {
    pub fn new(title: &str, page: &str, headers: &HeaderMap, token: &CsrfToken) -> Self {
        Self {
            title: title.to_owned(),
            tenant: session_tenant(headers)
                .map(|t| t.to_string())
                .unwrap_or_else(|| "00000000-0000-0000-0000-000000000001".to_string()),
            current_page: page.to_owned(),
            flash: Vec::new(),
            csrf: token.hidden_field(),
        }
    }

    pub fn with_flash(mut self, msg: FlashMessage) -> Self {
        self.flash.push(msg);
        self
    }
}
