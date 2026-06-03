use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::runtime_paths::machine_identity_path;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalMachineIdentity {
    pub machine_id: String,
    pub machine_label: String,
    #[serde(default)]
    pub legacy_machine_ids: Vec<String>,
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
        legacy_machine_ids: Vec::new(),
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
    let legacy_machine_ids =
        normalize_legacy_machine_ids(identity.legacy_machine_ids, &machine_id, &machine_label);

    LocalMachineIdentity {
        machine_id,
        machine_label,
        legacy_machine_ids,
    }
}

fn local_hostname() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn machine_label_alias_keys(value: &str) -> std::collections::HashSet<String> {
    let label = value.trim();
    if label.is_empty() {
        return std::collections::HashSet::new();
    }

    let lower = label.to_ascii_lowercase();
    let mut aliases = std::collections::HashSet::from([lower.clone()]);
    if !lower.ends_with(".local") {
        aliases.insert(format!("{lower}.local"));
    }
    aliases
}

fn normalize_legacy_machine_ids(
    values: Vec<String>,
    machine_id: &str,
    machine_label: &str,
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let machine_label_aliases = machine_label_alias_keys(machine_label);
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| {
            let key = value.to_ascii_lowercase();
            !value.is_empty() && value != machine_id && !machine_label_aliases.contains(&key)
        })
        .filter(|value| seen.insert(value.to_ascii_lowercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_machine_identity_preserves_explicit_legacy_ids() {
        let identity = normalize_machine_identity(LocalMachineIdentity {
            machine_id: "machine-a".to_string(),
            machine_label: "LIAOYIHAO-P".to_string(),
            legacy_machine_ids: vec![
                "dev-local".to_string(),
                "DEV-LOCAL".to_string(),
                "liaoyihao-p".to_string(),
                "liaoyihao-p.local".to_string(),
            ],
        });

        assert_eq!(identity.machine_id, "machine-a");
        assert_eq!(identity.machine_label, "LIAOYIHAO-P");
        assert_eq!(identity.legacy_machine_ids, vec!["dev-local".to_string()]);
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
