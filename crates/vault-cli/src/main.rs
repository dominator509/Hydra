use std::env;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use bridge_host::{EncryptedVault, DEFAULT_VAULT_PATH};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("hydra-vault error: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(|| anyhow::anyhow!(usage()))?;
    let path = vault_path();

    match command.as_str() {
        "set" => {
            let name = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("set requires a secret name\n{}", usage()))?;
            if args.next().is_some() {
                bail!("set accepts only a name; provide the value on stdin");
            }
            let mut value = String::new();
            io::stdin()
                .read_to_string(&mut value)
                .context("read secret value from stdin")?;
            let value = strip_terminal_newline(&value);
            let key = required_env("HYDRA_VAULT_KEY")?;
            let mut vault = load_or_create(&path, &key)?;
            vault.set(&name, value)?;
            vault.save(&path, &key)?;
            println!("updated {name}");
        }
        "get-names" | "names" => {
            if args.next().is_some() {
                bail!("get-names accepts no arguments");
            }
            let key = required_env("HYDRA_VAULT_KEY")?;
            let vault = EncryptedVault::load(&path, &key)?;
            for name in vault.names() {
                println!("{name}");
            }
        }
        "rotate" => {
            if args.next().is_some() {
                bail!("rotate accepts no arguments");
            }
            let old_key = required_env("HYDRA_VAULT_KEY")?;
            let next_key = required_env("HYDRA_VAULT_NEXT_KEY")?;
            let vault = EncryptedVault::load(&path, &old_key)?;
            vault.save(&path, &next_key)?;
            println!("vault rotated");
        }
        "backup" => {
            let destination = args
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("backup requires a destination\n{}", usage()))?;
            if args.next().is_some() {
                bail!("backup accepts only a destination path");
            }
            let key = required_env("HYDRA_VAULT_KEY")?;
            EncryptedVault::backup_to(&path, &destination, &key)?;
            println!("vault backup: ok");
        }
        "restore" => {
            let source = args
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("restore requires a source\n{}", usage()))?;
            if args.next().is_some() {
                bail!("restore accepts only a source path");
            }
            if env::var("HYDRA_VAULT_RESTORE_CONFIRM").as_deref() != Ok("restore") {
                bail!("set HYDRA_VAULT_RESTORE_CONFIRM=restore for an explicit vault restore");
            }
            let key = required_env("HYDRA_VAULT_KEY")?;
            EncryptedVault::restore_to(&source, &path, &key)?;
            println!("vault restore: ok");
        }
        "help" | "--help" | "-h" => println!("{}", usage()),
        other => bail!("unknown command '{other}'\n{}", usage()),
    }
    Ok(())
}

fn vault_path() -> PathBuf {
    env::var_os("HYDRA_VAULT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_VAULT_PATH))
}

fn required_env(name: &str) -> Result<String> {
    let value = env::var(name).with_context(|| format!("{name} is required"))?;
    if value.trim().is_empty() {
        bail!("{name} must not be empty");
    }
    Ok(value)
}

fn load_or_create(path: &PathBuf, key: &str) -> Result<EncryptedVault> {
    if path.exists() {
        Ok(EncryptedVault::load(path, key)?)
    } else {
        Ok(EncryptedVault::new())
    }
}

fn strip_terminal_newline(value: &str) -> &str {
    value
        .strip_suffix('\n')
        .map(|value| value.strip_suffix('\r').unwrap_or(value))
        .unwrap_or(value)
}

fn usage() -> &'static str {
    "usage: hydra-vault <set NAME|get-names|rotate|backup DESTINATION|restore SOURCE>\n\nset reads the secret value from stdin; backup and restore copy validated encrypted artifacts; values and keys are never accepted as arguments or printed."
}

#[cfg(test)]
mod tests {
    use super::strip_terminal_newline;

    #[test]
    fn strips_only_the_terminal_line_ending() {
        assert_eq!(strip_terminal_newline("value\n"), "value");
        assert_eq!(strip_terminal_newline("value\r\n"), "value");
        assert_eq!(strip_terminal_newline("value\nnext"), "value\nnext");
    }
}
