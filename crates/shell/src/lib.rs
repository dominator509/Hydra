mod csrf;
mod flash;
mod routes;

pub use csrf::CsrfToken;
pub use flash::FlashMessage;

pub fn router(state: fabric::AppState) -> axum::Router {
    routes::router(state)
}
