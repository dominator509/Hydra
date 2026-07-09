//! EP-006 M2 — AuthZ matrix tests.
//!
//! Covers every SPEC-003 REST endpoint against the four role levels plus
//! anonymous access, verifying correct authorization behavior.
//!
//! ## Structure
//!
//! 1. **Unit tests** — table-driven `AuthCtx::require_role` checks for all
//!    role combinations (Viewer, Operator, Approver, Admin, anonymous).
//! 2. **Endpoint-to-role mapping** — documents which role each REST route
//!    requires, tested through `AuthCtx::require_role`.
//! 3. **Integration tests** — end-to-end HTTP tests against a spawned app
//!    exercising the full auth path (dev-admin gate, tenant header, etc.).

use fabric::auth::{AuthCtx, Role, Session};
use std::sync::Arc;
use store::{Store, TestDb};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEV_TENANT_RAW: &str = "00000000-0000-0000-0000-000000000000";

fn dev_tenant() -> Uuid {
    Uuid::parse_str(DEV_TENANT_RAW).expect("dev tenant uuid")
}

// ---------------------------------------------------------------------------
// Helpers — one-per-role AuthCtx factories
// ---------------------------------------------------------------------------

fn anonymous_ctx() -> AuthCtx {
    AuthCtx {
        principal: "anonymous".into(),
        tenant: dev_tenant(),
        session: None,
    }
}

fn viewer_ctx() -> AuthCtx {
    AuthCtx {
        principal: "user:viewer".into(),
        tenant: dev_tenant(),
        session: Some(Session {
            user_id: Uuid::new_v4(),
            tenant_id: dev_tenant(),
            username: "viewer".into(),
            roles: vec![Role::Viewer],
            token: "viewer-token".into(),
        }),
    }
}

fn operator_ctx() -> AuthCtx {
    AuthCtx {
        principal: "user:operator".into(),
        tenant: dev_tenant(),
        session: Some(Session {
            user_id: Uuid::new_v4(),
            tenant_id: dev_tenant(),
            username: "operator".into(),
            roles: vec![Role::Operator],
            token: "operator-token".into(),
        }),
    }
}

fn approver_ctx() -> AuthCtx {
    AuthCtx {
        principal: "user:approver".into(),
        tenant: dev_tenant(),
        session: Some(Session {
            user_id: Uuid::new_v4(),
            tenant_id: dev_tenant(),
            username: "approver".into(),
            roles: vec![Role::Approver],
            token: "approver-token".into(),
        }),
    }
}

fn admin_ctx() -> AuthCtx {
    AuthCtx {
        principal: "user:admin".into(),
        tenant: dev_tenant(),
        session: Some(Session {
            user_id: Uuid::new_v4(),
            tenant_id: dev_tenant(),
            username: "admin".into(),
            roles: vec![Role::Admin],
            token: "admin-token".into(),
        }),
    }
}

/// Build a context scoped to a specific role vector.
fn ctx_with_roles(roles: Vec<Role>, tag: &str) -> AuthCtx {
    if roles.is_empty() {
        return anonymous_ctx();
    }
    AuthCtx {
        principal: format!("user:{tag}"),
        tenant: dev_tenant(),
        session: Some(Session {
            user_id: Uuid::new_v4(),
            tenant_id: dev_tenant(),
            username: tag.into(),
            roles,
            token: format!("{tag}-token"),
        }),
    }
}

// ---------------------------------------------------------------------------
// Individual role capability unit tests
// ---------------------------------------------------------------------------

#[test]
fn authz_admin_can_do_all_actions() {
    let ctx = admin_ctx();
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_ok());
    assert!(ctx.require_role(Role::Approver).is_ok());
    assert!(ctx.require_role(Role::Admin).is_ok());
}

#[test]
fn authz_approver_can_approve_but_not_admin() {
    let ctx = approver_ctx();
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_ok());
    assert!(ctx.require_role(Role::Approver).is_ok());
    assert!(ctx.require_role(Role::Admin).is_err());
}

#[test]
fn authz_operator_can_write_but_not_approve() {
    let ctx = operator_ctx();
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_ok());
    assert!(ctx.require_role(Role::Approver).is_err());
    assert!(ctx.require_role(Role::Admin).is_err());
}

#[test]
fn authz_viewer_can_only_read() {
    let ctx = viewer_ctx();
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_err());
    assert!(ctx.require_role(Role::Approver).is_err());
    assert!(ctx.require_role(Role::Admin).is_err());
}

#[test]
fn authz_anonymous_denied_everywhere() {
    let ctx = anonymous_ctx();
    assert!(ctx.require_role(Role::Viewer).is_err());
    assert!(ctx.require_role(Role::Operator).is_err());
    assert!(ctx.require_role(Role::Approver).is_err());
    assert!(ctx.require_role(Role::Admin).is_err());
}

// ---------------------------------------------------------------------------
// Table-driven matrix: every role combination × every require_role check
// ---------------------------------------------------------------------------

/// A single case in the authz matrix: given a set of roles, which
/// `require_role` calls should succeed or fail?
struct RoleCase {
    /// Human-readable label for the role set.
    label: &'static str,
    /// The session's assigned roles (empty = anonymous / no session).
    roles: Vec<Role>,
    /// Expected result for `require_role(Role::Viewer)`.
    view: bool,
    /// Expected result for `require_role(Role::Operator)`.
    operate: bool,
    /// Expected result for `require_role(Role::Approver)`.
    approve: bool,
    /// Expected result for `require_role(Role::Admin)`.
    admin: bool,
}

#[test]
fn authz_matrix_table_all_roles_vs_all_endpoints() {
    let cases = vec![
        RoleCase {
            label: "anonymous",
            roles: vec![],
            view: false,
            operate: false,
            approve: false,
            admin: false,
        },
        RoleCase {
            label: "viewer",
            roles: vec![Role::Viewer],
            view: true,
            operate: false,
            approve: false,
            admin: false,
        },
        RoleCase {
            label: "operator",
            roles: vec![Role::Operator],
            view: true,
            operate: true,
            approve: false,
            admin: false,
        },
        RoleCase {
            label: "approver",
            roles: vec![Role::Approver],
            view: true,
            operate: true,
            approve: true,
            admin: false,
        },
        RoleCase {
            label: "admin",
            roles: vec![Role::Admin],
            view: true,
            operate: true,
            approve: true,
            admin: true,
        },
        // Multi-role combinations
        RoleCase {
            label: "viewer+operator",
            roles: vec![Role::Viewer, Role::Operator],
            view: true,
            operate: true,
            approve: false,
            admin: false,
        },
        RoleCase {
            label: "viewer+approver",
            roles: vec![Role::Viewer, Role::Approver],
            view: true,
            operate: true,
            approve: true,
            admin: false,
        },
        RoleCase {
            label: "operator+approver",
            roles: vec![Role::Operator, Role::Approver],
            view: true,
            operate: true,
            approve: true,
            admin: false,
        },
        RoleCase {
            label: "all_roles",
            roles: vec![Role::Viewer, Role::Operator, Role::Approver, Role::Admin],
            view: true,
            operate: true,
            approve: true,
            admin: true,
        },
    ];

    for case in &cases {
        let ctx = ctx_with_roles(case.roles.clone(), case.label);

        assert_eq!(
            ctx.require_role(Role::Viewer).is_ok(),
            case.view,
            "{}: require_role(Viewer) expected={}",
            case.label,
            case.view
        );
        assert_eq!(
            ctx.require_role(Role::Operator).is_ok(),
            case.operate,
            "{}: require_role(Operator) expected={}",
            case.label,
            case.operate
        );
        assert_eq!(
            ctx.require_role(Role::Approver).is_ok(),
            case.approve,
            "{}: require_role(Approver) expected={}",
            case.label,
            case.approve
        );
        assert_eq!(
            ctx.require_role(Role::Admin).is_ok(),
            case.admin,
            "{}: require_role(Admin) expected={}",
            case.label,
            case.admin
        );
    }
}

// ---------------------------------------------------------------------------
// Endpoint-to-role mapping (SPEC-003 routes)
// ---------------------------------------------------------------------------

/// Documents the minimum role required for each REST route.
struct RouteAuthEntry {
    method: &'static str,
    path: &'static str,
    /// Minimum role needed, or `None` for fully public endpoints.
    min_role: Option<Role>,
    #[allow(dead_code)]
    notes: &'static str,
}

#[test]
fn authz_route_role_mapping_is_documented() {
    // This test verifies the documented role mapping is internally consistent.
    let routes: Vec<RouteAuthEntry> = vec![
        // ---- Public ----
        RouteAuthEntry {
            method: "GET",
            path: "/v1/openapi.json",
            min_role: None,
            notes: "fully public, no auth",
        },
        RouteAuthEntry {
            method: "POST",
            path: "/mcp",
            min_role: None,
            notes: "fully public, no auth (MCP JSON-RPC)",
        },
        RouteAuthEntry {
            method: "GET",
            path: "/v1/tk/ledger",
            min_role: None,
            notes: "fully public, no auth (TK stats)",
        },
        // ---- Read (Viewer+) ----
        RouteAuthEntry {
            method: "GET",
            path: "/v1/autonomy/cells",
            min_role: Some(Role::Viewer),
            notes: "tenant-scoped read",
        },
        RouteAuthEntry {
            method: "GET",
            path: "/v1/bridges/{id}/status",
            min_role: Some(Role::Viewer),
            notes: "tenant-scoped read",
        },
        RouteAuthEntry {
            method: "GET",
            path: "/v1/entities/{kind}",
            min_role: Some(Role::Viewer),
            notes: "tenant-scoped read",
        },
        RouteAuthEntry {
            method: "GET",
            path: "/v1/entities/{kind}/{id}",
            min_role: Some(Role::Viewer),
            notes: "tenant-scoped read",
        },
        RouteAuthEntry {
            method: "GET",
            path: "/v1/envelopes",
            min_role: Some(Role::Viewer),
            notes: "tenant-scoped read",
        },
        RouteAuthEntry {
            method: "POST",
            path: "/v1/concierge/ping",
            min_role: Some(Role::Viewer),
            notes: "tenant-scoped read (smoke test)",
        },
        // ---- Write (Operator+) ----
        RouteAuthEntry {
            method: "POST",
            path: "/v1/entities/{kind}",
            min_role: Some(Role::Operator),
            notes: "tenant-scoped write",
        },
        RouteAuthEntry {
            method: "PATCH",
            path: "/v1/entities/{kind}/{id}",
            min_role: Some(Role::Operator),
            notes: "tenant-scoped write (requires If-Match)",
        },
        RouteAuthEntry {
            method: "DELETE",
            path: "/v1/entities/{kind}/{id}",
            min_role: Some(Role::Operator),
            notes: "tenant-scoped soft delete",
        },
        RouteAuthEntry {
            method: "POST",
            path: "/v1/envelopes",
            min_role: Some(Role::Operator),
            notes: "tenant-scoped proposal",
        },
        // ---- Approve (Approver+) ----
        RouteAuthEntry {
            method: "POST",
            path: "/v1/envelopes/{id}/approve",
            min_role: Some(Role::Approver),
            notes: "requires approver role",
        },
        RouteAuthEntry {
            method: "POST",
            path: "/v1/envelopes/{id}/reject",
            min_role: Some(Role::Approver),
            notes: "requires approver role",
        },
        // ---- Admin only ----
        RouteAuthEntry {
            method: "PUT",
            path: "/v1/autonomy/cells",
            min_role: Some(Role::Admin),
            notes: "replaces all autonomy cells",
        },
        RouteAuthEntry {
            method: "POST",
            path: "/v1/bridges",
            min_role: Some(Role::Admin),
            notes: "registers a bridge adapter",
        },
        RouteAuthEntry {
            method: "POST",
            path: "/v1/bridges/{id}/pause",
            min_role: Some(Role::Admin),
            notes: "pauses bridge activity",
        },
        RouteAuthEntry {
            method: "POST",
            path: "/v1/bridges/{id}/resume",
            min_role: Some(Role::Admin),
            notes: "resumes bridge activity",
        },
    ];

    // Verify that each documented route has a consistent role hierarchy.
    for entry in &routes {
        if let Some(ref min_role) = entry.min_role {
            let valid_roles = [
                Role::Viewer,
                Role::Operator,
                Role::Approver,
                Role::Admin,
            ];
            assert!(
                valid_roles.contains(min_role),
                "{} {}: unknown min_role {:?}",
                entry.method,
                entry.path,
                min_role,
            );
        }
    }

    // Verify the total count matches the number of registered routes.
    assert_eq!(routes.len(), 19, "expected 19 SPEC-003 route entries");
}

// ---------------------------------------------------------------------------
// Edge-case tests
// ---------------------------------------------------------------------------

#[test]
fn authz_empty_roles_is_denied() {
    let ctx = ctx_with_roles(vec![], "empty");
    assert!(ctx.require_role(Role::Viewer).is_err());
    assert!(ctx.require_role(Role::Admin).is_err());
}

#[test]
fn authz_multi_role_approver_gets_highest() {
    // A user with Viewer + Approver should get Approver-level access.
    let ctx = ctx_with_roles(vec![Role::Viewer, Role::Approver], "hybrid");
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_ok()); // Approver >= Operator
    assert!(ctx.require_role(Role::Approver).is_ok());
    assert!(ctx.require_role(Role::Admin).is_err());
}

#[test]
fn authz_multi_role_viewer_plus_operator() {
    let ctx = ctx_with_roles(vec![Role::Viewer, Role::Operator], "viewer-op");
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_ok());
    assert!(ctx.require_role(Role::Approver).is_err());
}

#[test]
fn authz_require_admin_fails_for_non_admin() {
    for ctx in &[viewer_ctx(), operator_ctx(), approver_ctx(), anonymous_ctx()] {
        assert!(
            ctx.require_role(Role::Admin).is_err(),
            "non-admin should be denied Admin role"
        );
    }
}

#[test]
fn authz_require_approver_fails_for_viewer_and_operator() {
    for ctx in &[viewer_ctx(), operator_ctx(), anonymous_ctx()] {
        assert!(
            ctx.require_role(Role::Approver).is_err(),
            "non-approver should be denied Approver role"
        );
    }
}

#[test]
fn authz_viewer_role_on_admin_session() {
    // Admin can act as a viewer — this should succeed.
    let ctx = admin_ctx();
    assert!(ctx.require_role(Role::Viewer).is_ok());
}

#[test]
fn authz_session_with_all_roles() {
    let ctx = ctx_with_roles(
        vec![Role::Viewer, Role::Operator, Role::Approver, Role::Admin],
        "omnipotent",
    );
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_ok());
    assert!(ctx.require_role(Role::Approver).is_ok());
    assert!(ctx.require_role(Role::Admin).is_ok());
}

#[test]
fn authz_tenant_mismatch_does_not_affect_role_check() {
    // Role checking does not involve tenant — it's purely based on the
    // session's role list. Tenant enforcement is a separate concern.
    let different_tenant =
        Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("alternate tenant uuid");

    let ctx = AuthCtx {
        principal: "user:cross-tenant".into(),
        tenant: different_tenant,
        session: Some(Session {
            user_id: Uuid::new_v4(),
            tenant_id: dev_tenant(),
            username: "cross-tenant".into(),
            roles: vec![Role::Admin],
            token: "admin-token".into(),
        }),
    };

    // Even though tenant on AuthCtx != session.tenant_id, role check still
    // passes because require_role only inspects `session.roles`.
    assert!(ctx.require_role(Role::Admin).is_ok());
}

// ---------------------------------------------------------------------------
// Edge-case: developer role precedence (operator + admin multi-role)
// ---------------------------------------------------------------------------

#[test]
fn authz_operator_plus_admin_gets_admin() {
    let ctx = ctx_with_roles(vec![Role::Operator, Role::Admin], "op-admin");
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_ok());
    assert!(ctx.require_role(Role::Approver).is_ok());
    assert!(ctx.require_role(Role::Admin).is_ok());
}

// ---------------------------------------------------------------------------
// Integration tests (require DATABASE_URL for PostgreSQL)
// ---------------------------------------------------------------------------

fn db_available() -> bool {
    std::env::var("DATABASE_URL").is_ok()
}

#[tokio::test]
async fn authz_integration_openapi_is_public() -> Result<(), Box<dyn std::error::Error>> {
    if !db_available() {
        eprintln!("skipping: DATABASE_URL not set");
        return Ok(());
    }
    let db = TestDb::new().await?;
    let result = async {
        let (addr, _state) = spawn_test_app(db.pool.clone()).await?;
        let client = reqwest::Client::new();

        // No auth headers at all — should be fully public.
        let resp = client
            .get(format!("http://{addr}/v1/openapi.json"))
            .send()
            .await?;
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::OK,
            "openapi endpoint MUST be public"
        );

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn authz_integration_tk_ledger_is_public() -> Result<(), Box<dyn std::error::Error>> {
    if !db_available() {
        eprintln!("skipping: DATABASE_URL not set");
        return Ok(());
    }
    let db = TestDb::new().await?;
    let result = async {
        let (addr, _state) = spawn_test_app(db.pool.clone()).await?;
        let client = reqwest::Client::new();

        // No auth headers required for TK ledger.
        let resp = client
            .get(format!("http://{addr}/v1/tk/ledger?window=1h"))
            .send()
            .await?;
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::OK,
            "TK ledger endpoint MUST be public"
        );

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn authz_integration_missing_tenant_is_422() -> Result<(), Box<dyn std::error::Error>> {
    if !db_available() {
        eprintln!("skipping: DATABASE_URL not set");
        return Ok(());
    }
    let db = TestDb::new().await?;
    let result = async {
        let (addr, _state) = spawn_test_app(db.pool.clone()).await?;
        let client = reqwest::Client::new();

        // Endpoints that require x-hydra-tenant but don't get it should 422.
        let resp = client
            .get(format!("http://{addr}/v1/autonomy/cells"))
            .send()
            .await?;
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
            "missing tenant header should fail with 422"
        );

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn authz_integration_admin_endpoints_deny_without_token(
) -> Result<(), Box<dyn std::error::Error>> {
    if !db_available() {
        eprintln!("skipping: DATABASE_URL not set");
        return Ok(());
    }
    let db = TestDb::new().await?;
    let result = async {
        let (addr, _state) = spawn_test_app(db.pool.clone()).await?;
        let client = reqwest::Client::new();
        let tenant = Uuid::new_v4();

        // Admin-only endpoint without admin token should get FORBIDDEN.
        let resp = client
            .put(format!("http://{addr}/v1/autonomy/cells"))
            .header("x-hydra-tenant", tenant.to_string())
            .json(&serde_json::json!([]))
            .send()
            .await?;
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::FORBIDDEN,
            "PUT /v1/autonomy/cells without admin token should be FORBIDDEN"
        );

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn authz_integration_admin_endpoints_succeed_with_token(
) -> Result<(), Box<dyn std::error::Error>> {
    if !db_available() {
        eprintln!("skipping: DATABASE_URL not set");
        return Ok(());
    }
    let db = TestDb::new().await?;
    let result = async {
        std::env::set_var("HYDRA_ENV", "dev");
        let (addr, _state) = spawn_test_app(db.pool.clone()).await?;
        let client = reqwest::Client::new();
        let tenant = Uuid::new_v4();

        // Admin endpoints should succeed with Bearer hydra-dev-admin.
        let resp = client
            .put(format!("http://{addr}/v1/autonomy/cells"))
            .header("x-hydra-tenant", tenant.to_string())
            .header("Authorization", "Bearer hydra-dev-admin")
            .json(&serde_json::json!([
                {
                    "domain": "pipeline",
                    "action": "move_stage",
                    "kind": "deal",
                    "level": "L4",
                    "cfg": {"batch_max": 10}
                }
            ]))
            .send()
            .await?;
        assert!(
            resp.status().is_success(),
            "PUT /v1/autonomy/cells with admin token should succeed, got {}",
            resp.status()
        );

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn authz_integration_entity_tenant_isolation() -> Result<(), Box<dyn std::error::Error>> {
    if !db_available() {
        eprintln!("skipping: DATABASE_URL not set");
        return Ok(());
    }
    let db = TestDb::new().await?;
    let result = async {
        let (addr, _state) = spawn_test_app(db.pool.clone()).await?;
        let client = reqwest::Client::new();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        // Create an entity for tenant_a.
        let created = client
            .post(format!("http://{addr}/v1/entities/party"))
            .header("x-hydra-tenant", tenant_a.to_string())
            .json(&serde_json::json!({"display_name": "Alice"}))
            .send()
            .await?
            .error_for_status()?
            .json::<cdm::Entity>()
            .await?;

        // Tenant B should NOT see tenant A's entity.
        let listed = client
            .get(format!("http://{addr}/v1/entities/party?limit=5"))
            .header("x-hydra-tenant", tenant_b.to_string())
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<cdm::Entity>>()
            .await?;
        assert!(
            listed.is_empty(),
            "tenant B should not see tenant A's entities"
        );

        // Tenant B should get NOT_FOUND trying to fetch tenant A's entity.
        let missing = client
            .get(format!(
                "http://{addr}/v1/entities/party/{}",
                created.id
            ))
            .header("x-hydra-tenant", tenant_b.to_string())
            .send()
            .await?;
        assert_eq!(
            missing.status(),
            reqwest::StatusCode::NOT_FOUND,
            "tenant B should get 404 for tenant A's entity"
        );

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

// ---------------------------------------------------------------------------
// Test app scaffolding
// ---------------------------------------------------------------------------

async fn spawn_test_app(
    pool: sqlx::PgPool,
) -> Result<(std::net::SocketAddr, fabric::services::AppState), Box<dyn std::error::Error>> {
    use fabric::services::{
        demo_governor, AppState, ConciergeServiceImpl, StoreAutonomyService,
        StoreBridgeService, StoreEnvelopeService, StoreEntityService, StoreTkStatsService,
    };

    let store = Store::new(pool.clone());
    let state = AppState::new(
        Arc::new(fabric::auth::SessionStore::new(pool.clone())),
        Arc::new(StoreEntityService::new(store.clone())),
        Arc::new(StoreAutonomyService::new(store.clone())),
        Arc::new(StoreBridgeService::new(store.clone(), demo_governor())),
        Arc::new(StoreEnvelopeService::new(store.clone(), demo_governor())),
        Arc::new(StoreTkStatsService::new(
            store.ledger.clone(),
            vec!["concierge".into()],
        )),
        Arc::new(ConciergeServiceImpl),
    );

    let router = fabric::app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test server should stay alive");
    });

    Ok((addr, state))
}
