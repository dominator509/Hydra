use hydra_kernel::bridge_scheduler::slot_key;
use time::OffsetDateTime;
use uuid::Uuid;

#[test]
fn scheduler_slot_identity_is_deterministic() {
    let schedule_id = Uuid::from_u128(7);
    let due_at = OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("valid timestamp");
    assert_eq!(
        slot_key(schedule_id, due_at),
        "hydra.scheduler/00000000-0000-0000-0000-000000000007/1700000000"
    );
}
