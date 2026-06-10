use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::runtime_paths::machine_identity_path;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalMachineIdentity {
    pub machine_id: String,
    pub machine_label: String,
}

pub fn load_or_create_machine_identity() -> anyhow::Result<LocalMachineIdentity> {
    load_or_create_machine_identity_at(machine_identity_path()?)
}

fn load_or_create_machine_identity_at(path: PathBuf) -> anyhow::Result<LocalMachineIdentity> {
    if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        let parsed: LocalMachineIdentity = serde_json::from_str(&content)
            .map_err(|error| anyhow::anyhow!("读取本机 runtime 机器身份失败: {error}"))?;
        let normalized = normalize_machine_identity(parsed);
        save_machine_identity(&path, &normalized)?;
        return Ok(normalized);
    }

    let identity = normalize_machine_identity(LocalMachineIdentity {
        machine_id: uuid::Uuid::new_v4().to_string(),
        machine_label: local_hostname().unwrap_or_else(|| "AgentDash Local".to_string()),
    });
    save_machine_identity(&path, &identity)?;
    Ok(identity)
}

fn save_machine_identity(
    path: &std::path::Path,
    identity: &LocalMachineIdentity,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(identity)?;
    std::fs::write(path, content)?;
    Ok(())
}

fn normalize_machine_identity(identity: LocalMachineIdentity) -> LocalMachineIdentity {
    let machine_id = identity.machine_id.trim();
    let machine_id = if machine_id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        machine_id.to_string()
    };
    let machine_label = identity.machine_label.trim();
    let machine_label = if machine_label.is_empty() {
        local_hostname().unwrap_or_else(|| "AgentDash Local".to_string())
    } else {
        machine_label.to_string()
    };
    LocalMachineIdentity {
        machine_id,
        machine_label,
    }
}

fn local_hostname() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_machine_identity_trims_current_identity_fields() {
        let identity = normalize_machine_identity(LocalMachineIdentity {
            machine_id: " machine-a ".to_string(),
            machine_label: " LIAOYIHAO-P ".to_string(),
        });

        assert_eq!(identity.machine_id, "machine-a");
        assert_eq!(identity.machine_label, "LIAOYIHAO-P");
    }

    #[test]
    fn load_or_create_machine_identity_persists_to_local_owned_path() {
        let temp = tempfile::tempdir().expect("应能创建临时目录");
        let path = temp.path().join("machine-identity.json");

        let first =
            load_or_create_machine_identity_at(path.clone()).expect("首次应创建本机 runtime 身份");
        let second =
            load_or_create_machine_identity_at(path).expect("第二次应读取同一个本机 runtime 身份");

        assert_eq!(first.machine_id, second.machine_id);
        assert!(!first.machine_label.is_empty());
    }
}
