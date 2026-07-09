use fabric::auth::jwt::{TokenClaims, TokenScope, TokenService};
use uuid::Uuid;

#[test]
fn token_scope_parsing_all_valid() {
    let scopes = TokenScope::parse_all("read:cdm write:envelopes approve:envelopes admin:bridges admin:autonomy");
    assert_eq!(scopes.len(), 5);
}

#[test]
fn token_scope_parse_all_from_space_separated() {
    let scopes = TokenScope::parse_all("read:cdm write:envelopes");
    assert_eq!(scopes.len(), 2);
}

#[test]
fn token_parse_ignores_unknown_scopes() {
    let scopes = TokenScope::parse_all("read:cdm unknown:scope admin:bridges");
    assert_eq!(scopes.len(), 2);
}

#[test]
fn token_roundtrip_sign_and_verify() {
    let service = TokenService::new(b"test-secret-32-bytes-long-key!!".to_vec());
    let claims = TokenClaims::new(
        "user:admin".into(),
        Uuid::nil(),
        &[TokenScope::ReadCdm, TokenScope::WriteEnvelopes, TokenScope::AdminBridges],
        1,
    );
    let token = service.sign(&claims).expect("signing should succeed");
    let verified = service.verify(&token).expect("verification should succeed");

    assert_eq!(verified.sub, "user:admin");
    assert_eq!(verified.aud, Uuid::nil());
}

#[test]
fn token_verification_fails_with_wrong_secret() {
    let service1 = TokenService::new(b"secret-key-number-one-32bytes!!".to_vec());
    let service2 = TokenService::new(b"secret-key-number-two-32bytes!!".to_vec());

    let token = service1.sign(&TokenClaims::new(
        "user:test".into(), Uuid::nil(), &[TokenScope::ReadCdm], 1,
    )).expect("sign");

    assert!(service2.verify(&token).is_err());
}

#[test]
fn token_scope_none_requested_returns_empty() {
    let scopes = TokenScope::parse_all("");
    assert!(scopes.is_empty());
}

#[test]
fn token_as_str_roundtrip() {
    let all = TokenScope::parse_all("read:cdm write:envelopes approve:envelopes admin:bridges admin:autonomy");
    for scope in &all {
        let roundtripped = TokenScope::parse_all(scope.as_str());
        assert_eq!(roundtripped.len(), 1);
    }
}

#[test]
fn token_iat_and_exp_are_valid() {
    let claims = TokenClaims::new(
        "user:test".into(), Uuid::nil(), &[], 1,
    );
    assert!(claims.iat > 0);
    assert!(claims.exp > claims.iat);
    assert_eq!(claims.exp - claims.iat, 3600);
}

#[test]
fn token_with_24h_expiry() {
    let claims = TokenClaims::new(
        "user:test".into(), Uuid::nil(), &[], 24,
    );
    assert_eq!(claims.exp - claims.iat, 86400);
}
