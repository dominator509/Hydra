use std::env;
use std::io::{self, Read};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use store::{
    BindingStatus, BridgeScheduleState, NewBridgeSyncSchedule, NewExternalTenantBinding,
    NewOperatorUser, Store,
};
use uuid::Uuid;

const ADMIN_CONFIRMATION: &str = "I_UNDERSTAND";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("hydra-admin error: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<()> {
    runtime()?.block_on(async_run())
}

async fn async_run() -> Result<()> {
    let mut args = env::args().skip(1);
    let group = args.next().ok_or_else(|| anyhow::anyhow!(usage()))?;
    let command = args.next().ok_or_else(|| anyhow::anyhow!(usage()))?;

    match (group.as_str(), command.as_str()) {
        ("user", "create") => {
            let tenant_id = parse_uuid(&mut args, "tenant_id")?;
            let username = next_arg(&mut args, "username")?;
            let role = next_arg(&mut args, "role")?;
            let display_name = next_arg(&mut args, "display_name")?;
            reject_extra(&mut args)?;
            require_confirmation()?;
            let password = read_secret("operator password")?;
            let password_hash = fabric::auth::password::hash_password(&password)
                .map_err(|error| anyhow::anyhow!("hash operator password: {error}"))?;
            let store = connect_store().await?;
            let user = store
                .operators
                .create(NewOperatorUser {
                    tenant_id,
                    username,
                    password_hash,
                    display_name: Some(display_name),
                    role,
                })
                .await?;
            println!(
                "created operator id={} tenant_id={} username={} role={} disabled={}",
                user.id,
                user.tenant_id,
                user.username,
                user.role,
                user.disabled_at.is_some()
            );
        }
        ("user", "list") => {
            let tenant_id = parse_uuid(&mut args, "tenant_id")?;
            reject_extra(&mut args)?;
            let store = connect_store().await?;
            for user in store.operators.list_for_tenant(tenant_id).await? {
                println!(
                    "id={} tenant_id={} username={} role={} disabled={}",
                    user.id,
                    user.tenant_id,
                    user.username,
                    user.role,
                    user.disabled_at.is_some()
                );
            }
        }
        ("user", "status") => {
            let tenant_id = parse_uuid(&mut args, "tenant_id")?;
            let user_id = parse_uuid(&mut args, "user_id")?;
            let status = next_arg(&mut args, "enabled|disabled")?;
            reject_extra(&mut args)?;
            require_confirmation()?;
            let disabled = parse_disabled(&status)?;
            let store = connect_store().await?;
            let user = store
                .operators
                .set_disabled(tenant_id, user_id, disabled)
                .await?;
            println!(
                "updated operator id={} tenant_id={} username={} role={} disabled={}",
                user.id,
                user.tenant_id,
                user.username,
                user.role,
                user.disabled_at.is_some()
            );
        }
        ("binding", "create") => {
            let provider = next_arg(&mut args, "provider")?;
            let external_tenant_id = next_arg(&mut args, "external_tenant_id")?;
            let external_business_id = next_arg(&mut args, "external_business_id")?;
            let hydra_tenant_id = parse_uuid(&mut args, "hydra_tenant_id")?;
            reject_extra(&mut args)?;
            require_confirmation()?;
            let store = connect_store().await?;
            let binding = store
                .external_bindings
                .create(NewExternalTenantBinding {
                    provider,
                    external_tenant_id,
                    external_business_id,
                    hydra_tenant_id,
                })
                .await?;
            println!(
                "created binding id={} provider={} external_tenant_id={} external_business_id={} hydra_tenant_id={} status={}",
                binding.id,
                binding.provider,
                binding.external_tenant_id,
                binding.external_business_id,
                binding.hydra_tenant_id,
                binding.status.as_str()
            );
        }
        ("binding", "status") => {
            let binding_id = parse_uuid(&mut args, "binding_id")?;
            let status = parse_binding_status(&next_arg(&mut args, "active|disabled|revoked")?)?;
            reject_extra(&mut args)?;
            require_confirmation()?;
            let store = connect_store().await?;
            let binding = store
                .external_bindings
                .set_status(binding_id, status)
                .await?;
            println!(
                "updated binding id={} provider={} external_tenant_id={} external_business_id={} hydra_tenant_id={} status={}",
                binding.id,
                binding.provider,
                binding.external_tenant_id,
                binding.external_business_id,
                binding.hydra_tenant_id,
                binding.status.as_str()
            );
        }
        ("autonomy", "freeze") => {
            let tenant_id = parse_uuid(&mut args, "tenant_id")?;
            let reason = next_arg(&mut args, "reason")?;
            reject_extra(&mut args)?;
            require_confirmation()?;
            let store = connect_store().await?;
            let state = store
                .autonomy
                .set_frozen(tenant_id, true, Some(&reason), "local:hydra-admin")
                .await?;
            print_freeze_state(&state);
        }
        ("autonomy", "thaw") => {
            let tenant_id = parse_uuid(&mut args, "tenant_id")?;
            reject_extra(&mut args)?;
            require_confirmation()?;
            let store = connect_store().await?;
            let state = store
                .autonomy
                .set_frozen(tenant_id, false, None, "local:hydra-admin")
                .await?;
            print_freeze_state(&state);
        }
        ("autonomy", "status") => {
            let tenant_id = parse_uuid(&mut args, "tenant_id")?;
            reject_extra(&mut args)?;
            let store = connect_store().await?;
            let state = store.autonomy.freeze_status(tenant_id).await?;
            print_freeze_state(&state);
        }
        ("schedule", "create") => {
            let tenant_id = parse_uuid(&mut args, "tenant_id")?;
            let adapter_id = next_arg(&mut args, "adapter_id")?;
            let kind = next_arg(&mut args, "kind")?;
            let interval_seconds = parse_i64(&mut args, "interval_seconds")?;
            let page_limit = parse_i32(&mut args, "page_limit")?;
            reject_extra(&mut args)?;
            require_confirmation()?;
            let store = connect_store().await?;
            let schedule = store
                .bridge_schedules
                .create(NewBridgeSyncSchedule {
                    tenant_id,
                    adapter_id,
                    kind,
                    interval_seconds,
                    page_limit,
                })
                .await?;
            print_schedule(&schedule);
        }
        ("schedule", "list") => {
            let tenant_id = parse_uuid(&mut args, "tenant_id")?;
            reject_extra(&mut args)?;
            let store = connect_store().await?;
            for schedule in store.bridge_schedules.list_for_tenant(tenant_id).await? {
                print_schedule(&schedule);
            }
        }
        ("schedule", "status") => {
            let tenant_id = parse_uuid(&mut args, "tenant_id")?;
            let schedule_id = parse_uuid(&mut args, "schedule_id")?;
            let status = next_arg(&mut args, "enabled|disabled")?;
            reject_extra(&mut args)?;
            require_confirmation()?;
            let state = match status.as_str() {
                "enabled" => BridgeScheduleState::Enabled,
                "disabled" => BridgeScheduleState::Disabled,
                _ => bail!("status must be enabled or disabled"),
            };
            let store = connect_store().await?;
            let schedule = store
                .bridge_schedules
                .set_state(tenant_id, schedule_id, state)
                .await?;
            print_schedule(&schedule);
        }
        ("help", _) | ("--help", _) | ("-h", _) => println!("{}", usage()),
        _ => bail!("unknown command\n{}", usage()),
    }
    Ok(())
}

async fn connect_store() -> Result<Store> {
    let database_url = env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    Ok(Store::connect(&database_url, 2).await?)
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("construct admin runtime")
}

fn require_confirmation() -> Result<()> {
    if env::var("HYDRA_ADMIN_CONFIRM").ok().as_deref() != Some(ADMIN_CONFIRMATION) {
        bail!("mutation requires HYDRA_ADMIN_CONFIRM={ADMIN_CONFIRMATION}");
    }
    Ok(())
}

fn read_secret(label: &str) -> Result<String> {
    let mut value = String::new();
    io::stdin()
        .read_to_string(&mut value)
        .with_context(|| format!("read {label} from stdin"))?;
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.len() > 512 {
        bail!("{label} must be non-empty and at most 512 bytes");
    }
    Ok(value.to_owned())
}

fn parse_uuid(args: &mut impl Iterator<Item = String>, name: &str) -> Result<Uuid> {
    let value = next_arg(args, name)?;
    Uuid::parse_str(&value).with_context(|| format!("{name} must be a UUID"))
}

fn parse_i64(args: &mut impl Iterator<Item = String>, name: &str) -> Result<i64> {
    let value = next_arg(args, name)?;
    value
        .parse::<i64>()
        .with_context(|| format!("{name} must be an integer"))
}

fn parse_i32(args: &mut impl Iterator<Item = String>, name: &str) -> Result<i32> {
    let value = next_arg(args, name)?;
    value
        .parse::<i32>()
        .with_context(|| format!("{name} must be an integer"))
}

fn next_arg(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String> {
    args.next().ok_or_else(|| anyhow::anyhow!("missing {name}"))
}

fn reject_extra(args: &mut impl Iterator<Item = String>) -> Result<()> {
    if let Some(extra) = args.next() {
        bail!("unexpected argument '{extra}'");
    }
    Ok(())
}

fn parse_disabled(value: &str) -> Result<bool> {
    match value {
        "enabled" => Ok(false),
        "disabled" => Ok(true),
        _ => bail!("status must be enabled or disabled"),
    }
}

fn parse_binding_status(value: &str) -> Result<BindingStatus> {
    match value {
        "active" => Ok(BindingStatus::Active),
        "disabled" => Ok(BindingStatus::Disabled),
        "revoked" => Ok(BindingStatus::Revoked),
        _ => bail!("binding status must be active, disabled, or revoked"),
    }
}

fn print_freeze_state(state: &store::AutonomyFreeze) {
    println!(
        "autonomy tenant_id={} status={} actor={} reason={}",
        state.tenant_id,
        if state.frozen { "frozen" } else { "active" },
        state.actor,
        state.reason.as_deref().unwrap_or("-")
    );
}

fn print_schedule(schedule: &store::BridgeSyncSchedule) {
    println!(
        "schedule id={} tenant_id={} adapter_id={} kind={} interval_seconds={} page_limit={} enabled={} next_due_at={} last_envelope_id={} last_error={}",
        schedule.id,
        schedule.tenant_id,
        schedule.adapter_id,
        schedule.kind,
        schedule.interval_seconds,
        schedule.page_limit,
        schedule.enabled,
        schedule.next_due_at,
        schedule
            .last_envelope_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_owned()),
        schedule.last_error.as_deref().unwrap_or("-")
    );
}

fn usage() -> &'static str {
    "usage:
  hydra-admin user create TENANT_ID USERNAME ROLE DISPLAY_NAME < password.txt
  hydra-admin user list TENANT_ID
  hydra-admin user status TENANT_ID USER_ID enabled|disabled
  hydra-admin binding create PROVIDER EXTERNAL_TENANT_ID EXTERNAL_BUSINESS_ID HYDRA_TENANT_ID
  hydra-admin binding status BINDING_ID active|disabled|revoked
  hydra-admin autonomy freeze TENANT_ID REASON
  hydra-admin autonomy thaw TENANT_ID
  hydra-admin autonomy status TENANT_ID
  hydra-admin schedule create TENANT_ID ADAPTER_ID KIND INTERVAL_SECONDS PAGE_LIMIT
  hydra-admin schedule list TENANT_ID
  hydra-admin schedule status TENANT_ID SCHEDULE_ID enabled|disabled

Mutations require HYDRA_ADMIN_CONFIRM=I_UNDERSTAND and DATABASE_URL.
Autonomy status and schedule list are read-only and print no secrets."
}

#[cfg(test)]
mod tests {
    use super::{parse_binding_status, parse_disabled, ADMIN_CONFIRMATION};

    #[test]
    fn status_parsers_fail_closed() {
        assert_eq!(parse_disabled("enabled").ok(), Some(false));
        assert_eq!(parse_disabled("disabled").ok(), Some(true));
        assert!(parse_disabled("maybe").is_err());
        assert!(parse_binding_status("deleted").is_err());
    }

    #[test]
    fn confirmation_constant_is_explicit() {
        assert_eq!(ADMIN_CONFIRMATION, "I_UNDERSTAND");
    }

    #[test]
    fn usage_never_requests_password_as_an_argument() {
        assert!(super::usage().contains("< password.txt"));
        assert!(!super::usage().contains("PASSWORD"));
    }

    #[test]
    fn secret_input_trims_only_terminal_line_endings() {
        let value = "secret\n".trim_end_matches(['\r', '\n']);
        assert_eq!(value, "secret");
        let value = "secret\nnext".trim_end_matches(['\r', '\n']);
        assert_eq!(value, "secret\nnext");
    }
}
