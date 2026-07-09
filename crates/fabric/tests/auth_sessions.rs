use std::str::FromStr;

use fabric::auth::Role;

#[test]
fn role_from_str_valid() {
    assert_eq!(Role::from_str("viewer").ok(), Some(Role::Viewer));
    assert_eq!(Role::from_str("operator").ok(), Some(Role::Operator));
    assert_eq!(Role::from_str("approver").ok(), Some(Role::Approver));
    assert_eq!(Role::from_str("admin").ok(), Some(Role::Admin));
    assert!(Role::from_str("superuser").is_err());
}

#[test]
fn role_ordering() {
    assert!(Role::Admin.require(&Role::Viewer));
    assert!(Role::Admin.require(&Role::Admin));
    assert!(!Role::Viewer.require(&Role::Operator));
    assert!(Role::Approver.require(&Role::Operator));
    assert!(!Role::Operator.require(&Role::Approver));
}

#[test]
fn role_as_str_roundtrip() {
    for role in &[Role::Viewer, Role::Operator, Role::Approver, Role::Admin] {
        assert_eq!(Role::from_str(role.as_str()).ok(), Some(role.clone()));
    }
}
