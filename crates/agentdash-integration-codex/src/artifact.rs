use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const MANIFEST_NAME: &str = "hooks/hooks.json";
const BRIDGE_NAME: &str = "hooks/agentdash-hook-bridge.js";
const SCHEMA_NAME: &str = "hooks/agentdash-hook-invocation.schema.json";
const PLUGIN_MANIFEST_NAME: &str = ".codex-plugin/plugin.json";
const ADAPTER_REVISION: &str = "agentdash-codex-hook-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookArtifactPlan {
    pub plan_revision: u64,
    pub plan_digest: String,
    pub required_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MaterializedHookArtifact {
    pub digest: String,
    pub root: PathBuf,
    pub manifest: PathBuf,
}

#[derive(Debug, Error)]
pub(crate) enum ArtifactError {
    #[error("hook artifact root must be absolute")]
    RelativeRoot,
    #[error("hook artifact I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("hook artifact serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("immutable hook artifact at {path} does not match its digest")]
    DigestCollision { path: String },
}

pub(crate) fn materialize_hook_artifact(
    base: &Path,
    plan: &HookArtifactPlan,
) -> Result<MaterializedHookArtifact, ArtifactError> {
    if !base.is_absolute() {
        return Err(ArtifactError::RelativeRoot);
    }

    let bridge = bridge_source();
    let schema = invocation_schema();
    let manifest = manifest_value(plan);
    let plugin_manifest = plugin_manifest_value();
    let manifest_bytes = canonical_json(&manifest)?;
    let schema_bytes = canonical_json(&schema)?;
    let plugin_manifest_bytes = canonical_json(&plugin_manifest)?;
    let digest = artifact_digest(
        &manifest_bytes,
        bridge.as_bytes(),
        &schema_bytes,
        &plugin_manifest_bytes,
    );
    let digest_path = digest.replace(':', "-");
    let root = base.join(&digest_path);
    let manifest_path = root.join(MANIFEST_NAME);

    if root.exists() {
        verify_existing(&root, &digest)?;
        return Ok(MaterializedHookArtifact {
            digest,
            root,
            manifest: manifest_path,
        });
    }

    let staging = base.join(format!(".{digest_path}.staging-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(staging.join("hooks"))?;
    fs::create_dir_all(staging.join(".codex-plugin"))?;
    fs::write(staging.join(MANIFEST_NAME), &manifest_bytes)?;
    fs::write(staging.join(BRIDGE_NAME), bridge.as_bytes())?;
    fs::write(staging.join(SCHEMA_NAME), &schema_bytes)?;
    fs::write(staging.join(PLUGIN_MANIFEST_NAME), &plugin_manifest_bytes)?;
    fs::write(staging.join("artifact.digest"), digest.as_bytes())?;
    fs::create_dir_all(base)?;
    match fs::rename(&staging, &root) {
        Ok(()) => {}
        Err(_) if root.exists() => {
            fs::remove_dir_all(&staging)?;
        }
        Err(error) => return Err(error.into()),
    }

    verify_existing(&root, &digest)?;
    Ok(MaterializedHookArtifact {
        digest,
        root,
        manifest: manifest_path,
    })
}

fn verify_existing(root: &Path, expected: &str) -> Result<(), ArtifactError> {
    let manifest = fs::read(root.join(MANIFEST_NAME))?;
    let bridge = fs::read(root.join(BRIDGE_NAME))?;
    let schema = fs::read(root.join(SCHEMA_NAME))?;
    let plugin_manifest = fs::read(root.join(PLUGIN_MANIFEST_NAME))?;
    let actual = artifact_digest(&manifest, &bridge, &schema, &plugin_manifest);
    let marker = fs::read_to_string(root.join("artifact.digest"))?;
    if actual != expected || marker != expected {
        return Err(ArtifactError::DigestCollision {
            path: root.display().to_string(),
        });
    }
    Ok(())
}

fn artifact_digest(manifest: &[u8], bridge: &[u8], schema: &[u8], plugin: &[u8]) -> String {
    let mut hasher = Sha256::new();
    for (name, bytes) in [
        (MANIFEST_NAME, manifest),
        (BRIDGE_NAME, bridge),
        (SCHEMA_NAME, schema),
        (PLUGIN_MANIFEST_NAME, plugin),
    ] {
        hasher.update((name.len() as u64).to_be_bytes());
        hasher.update(name.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    hasher.update(ADAPTER_REVISION.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn plugin_manifest_value() -> serde_json::Value {
    serde_json::json!({
        "name": "agentdash-runtime-hooks",
        "version": "1.0.0",
        "description": "Immutable AgentDash native hook bridge",
        "hooks": "./hooks/hooks.json"
    })
}

fn canonical_json(value: &serde_json::Value) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(value)
}

fn manifest_value(plan: &HookArtifactPlan) -> serde_json::Value {
    serde_json::json!({
        "version": 1,
        "hooks": hook_groups(&format!("node ${{PLUGIN_ROOT}}/{}", BRIDGE_NAME), plan.required_timeout_ms),
        "agentdash": {
            "planRevision": plan.plan_revision,
            "planDigest": plan.plan_digest,
            "requiredTimeoutMs": plan.required_timeout_ms,
            "adapterRevision": ADAPTER_REVISION
        }
    })
}

pub(crate) fn native_hook_config(
    artifact: &MaterializedHookArtifact,
    timeout_ms: u64,
) -> serde_json::Value {
    let command = format!("node \"{}\"", artifact.root.join(BRIDGE_NAME).display());
    serde_json::json!({ "hooks": hook_groups(&command, timeout_ms) })
}

fn hook_groups(command: &str, timeout_ms: u64) -> serde_json::Value {
    let group = || serde_json::json!([{ "matcher": "", "hooks": [{ "type": "command", "command": command, "timeout": timeout_ms }] }]);
    serde_json::json!({
        "PreToolUse": group(), "PermissionRequest": group(), "PostToolUse": group(),
        "PreCompact": group(), "PostCompact": group(), "SessionStart": group(),
        "UserPromptSubmit": group(), "Stop": group()
    })
}

fn invocation_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["hook_event_name"],
        "properties": { "hook_event_name": { "type": "string" } },
        "additionalProperties": true
    })
}

fn bridge_source() -> &'static str {
    r#"#!/usr/bin/env node
'use strict';
const fs = require('fs');
const path = require('path');
const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, 'hooks.json'), 'utf8'));
let input = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', chunk => input += chunk);
process.stdin.on('end', async () => {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), Number(manifest.agentdash.requiredTimeoutMs));
  try {
    const endpoint = process.env.AGENTDASH_HOOK_ENDPOINT;
    if (!endpoint) throw new Error('AGENTDASH_HOOK_ENDPOINT is not configured');
    const response = await fetch(endpoint, {
      method: 'POST', headers: {'content-type': 'application/json'}, body: input, signal: controller.signal
    });
    const body = await response.text();
    if (!response.ok) throw new Error(`callback ${response.status}: ${body}`);
    process.stdout.write(body);
  } catch (error) {
    process.stderr.write(String(error));
    process.exitCode = 2;
  } finally { clearTimeout(timeout); }
});
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> HookArtifactPlan {
        HookArtifactPlan {
            plan_revision: 7,
            plan_digest: "plan-7".to_string(),
            required_timeout_ms: 12_000,
        }
    }

    #[test]
    fn digest_covers_bridge_content_and_materialization_is_immutable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let first = materialize_hook_artifact(temp.path(), &plan()).expect("materialize");
        let second = materialize_hook_artifact(temp.path(), &plan()).expect("reuse");
        assert_eq!(first.digest, second.digest);
        assert_eq!(first.root, second.root);
        assert!(first.manifest.exists());
        assert!(first.root.join(PLUGIN_MANIFEST_NAME).exists());
        let manifest = fs::read_to_string(&first.manifest).expect("manifest");
        assert!(manifest.contains("PreToolUse"));
        assert!(!manifest.contains("bypass_hook_trust"));

        fs::write(first.root.join(BRIDGE_NAME), "replaced").expect("replace fixture");
        assert!(matches!(
            materialize_hook_artifact(temp.path(), &plan()),
            Err(ArtifactError::DigestCollision { .. })
        ));
    }

    #[test]
    fn digest_path_is_independent_of_linked_worktree_location() {
        let a = tempfile::tempdir().expect("tempdir a");
        let b = tempfile::tempdir().expect("tempdir b");
        let left = materialize_hook_artifact(a.path(), &plan()).expect("left");
        let right = materialize_hook_artifact(b.path(), &plan()).expect("right");
        assert_eq!(left.digest, right.digest);
        assert_ne!(left.root, right.root);
        let digest_path = left.digest.replace(':', "-");
        assert_eq!(
            left.root.file_name().and_then(|name| name.to_str()),
            Some(digest_path.as_str())
        );
    }

    #[test]
    fn concurrent_materialization_converges_on_one_verified_artifact() {
        use std::sync::{Arc, Barrier};

        let temp = tempfile::tempdir().expect("tempdir");
        let barrier = Arc::new(Barrier::new(4));
        let workers = (0..4)
            .map(|_| {
                let root = temp.path().to_path_buf();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    materialize_hook_artifact(&root, &plan()).expect("materialize")
                })
            })
            .collect::<Vec<_>>();
        let artifacts = workers
            .into_iter()
            .map(|worker| worker.join().expect("worker"))
            .collect::<Vec<_>>();
        assert!(artifacts.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
