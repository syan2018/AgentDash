use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::manager::digest_bytes;
use super::*;
use crate::extensions::artifact_cache::ExtensionArtifactCacheEntry;

#[tokio::test]
async fn local_hello_profile_executes_in_host() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_package(temp.path(), profile_bundle(), true, true)
        .await
        .expect("package");
    let manager = test_manager(temp.path());

    let health = manager
        .activate_dev_directory(&package_dir, activation())
        .await
        .expect("activate");
    assert_eq!(health.action_keys, vec!["local-hello.profile".to_string()]);

    let result = manager
        .invoke_action("local-hello.profile", Value::Null)
        .await
        .expect("invoke");
    assert_eq!(result["backend_id"], "backend-1");
    assert_eq!(result["project_id"], "project-1");
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn reload_updates_action_handler() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_package(temp.path(), version_bundle(1), true, true)
        .await
        .expect("package");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(&package_dir, activation())
        .await
        .expect("activate");
    assert_eq!(
        manager
            .invoke_action("local-hello.profile", Value::Null)
            .await
            .expect("invoke")["version"],
        1
    );

    write_bundle(&package_dir, version_bundle(2))
        .await
        .expect("rewrite bundle");
    manager
        .reload_dev_directory(&package_dir, activation())
        .await
        .expect("reload");
    assert_eq!(
        manager
            .invoke_action("local-hello.profile", Value::Null)
            .await
            .expect("invoke")["version"],
        2
    );
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn permission_denied_when_local_profile_is_not_declared() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_package(temp.path(), profile_bundle(), false, false)
        .await
        .expect("package");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(&package_dir, activation())
        .await
        .expect("activate");
    let err = manager
        .invoke_action("local-hello.profile", Value::Null)
        .await
        .expect_err("permission denied");
    assert!(err.to_string().contains("local.profile.read"));
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn permission_denied_when_action_local_profile_is_not_declared() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_package(temp.path(), profile_bundle(), true, false)
        .await
        .expect("package");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(&package_dir, activation())
        .await
        .expect("activate");
    let err = manager
        .invoke_action("local-hello.profile", Value::Null)
        .await
        .expect_err("permission denied");
    assert!(err.to_string().contains("local.profile.read"));
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn packaged_directory_verifies_bundle_digest() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_package(temp.path(), version_bundle(7), true, true)
        .await
        .expect("package");
    let cache_entry = ExtensionArtifactCacheEntry {
        cache_key: "cache-key".to_string(),
        archive_path: temp.path().join("archive.agentdash-extension.tgz"),
        unpacked_dir: package_dir,
    };
    let manager = test_manager(temp.path());
    manager
        .activate_cached_artifact(&cache_entry, activation())
        .await
        .expect("activate packaged");
    assert_eq!(
        manager
            .invoke_action("local-hello.profile", Value::Null)
            .await
            .expect("invoke")["version"],
        7
    );
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn action_exception_does_not_stop_host_process() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_package(temp.path(), throwing_bundle(), true, true)
        .await
        .expect("package");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(&package_dir, activation())
        .await
        .expect("activate");
    let err = manager
        .invoke_action("local-hello.profile", Value::Null)
        .await
        .expect_err("action error");
    assert!(err.to_string().contains("boom"));
    let health = manager.health().await.expect("health");
    assert!(health.active);
    manager.stop().await.expect("stop");
}

fn test_manager(root: &Path) -> LocalExtensionHostManager {
    LocalExtensionHostManager::new(LocalTsExtensionHostConfig {
        node_command: "node".to_string(),
        runner_dir: root.join("runner"),
    })
}

fn activation() -> LocalExtensionHostActivation {
    LocalExtensionHostActivation {
        extension_key: "local-hello".to_string(),
        backend_id: "backend-1".to_string(),
        project_id: Some("project-1".to_string()),
        session_id: Some("session-1".to_string()),
        workspace_roots: vec![PathBuf::from("C:/secret/workspace")],
    }
}

async fn write_package(
    root: &Path,
    bundle: String,
    include_top_level_permission: bool,
    include_action_permission: bool,
) -> anyhow::Result<PathBuf> {
    let package_dir = root.join("package");
    tokio::fs::create_dir_all(package_dir.join("dist")).await?;
    write_bundle(&package_dir, bundle.clone()).await?;
    let digest = digest_bytes(bundle.as_bytes());
    let permissions = if include_top_level_permission {
        json!([{ "kind": "local_profile", "access": "read" }])
    } else {
        json!([])
    };
    let action_permissions = if include_action_permission {
        json!(["local.profile.read"])
    } else {
        json!([])
    };
    let manifest = json!({
        "manifest_version": "2",
        "extension_id": "local-hello",
        "package": { "name": "@agentdash/local-hello", "version": "0.1.0" },
        "asset_version": "0.1.0",
        "runtime_actions": [{
            "action_key": "local-hello.profile",
            "kind": "session_runtime",
            "description": "Read local profile",
            "input_schema": {},
            "output_schema": {},
            "permissions": action_permissions,
        }],
        "permissions": permissions,
        "bundles": [{
            "kind": "extension_host",
            "entry": "dist/extension.js",
            "digest": digest,
        }],
    });
    tokio::fs::write(
        package_dir.join("agentdash.extension.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .await?;
    Ok(package_dir)
}

async fn write_bundle(package_dir: &Path, bundle: String) -> anyhow::Result<()> {
    tokio::fs::write(package_dir.join("dist").join("extension.js"), bundle).await?;
    Ok(())
}

fn profile_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: "local-hello.profile",
      kind: "session_runtime",
      description: "Read local profile",
      async invoke() {
        return await ctx.api.local.getProfile();
      },
    });
  },
};
"#
    .to_string()
}

fn version_bundle(version: i32) -> String {
    format!(
        r#"
export default {{
  activate(ctx) {{
    ctx.runtime.registerAction({{
      action_key: "local-hello.profile",
      kind: "session_runtime",
      description: "Version",
      invoke() {{
        return {{ version: {version} }};
      }},
    }});
  }},
}};
"#
    )
}

fn throwing_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: "local-hello.profile",
      kind: "session_runtime",
      description: "Throw",
      invoke() {
        throw new Error("boom");
      },
    });
  },
};
"#
    .to_string()
}
