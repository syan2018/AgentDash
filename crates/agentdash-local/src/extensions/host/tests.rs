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
async fn protocol_channel_registers_and_self_invokes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_channel_echo_package(temp.path())
        .await
        .expect("package");
    let manager = test_manager(temp.path());
    let health = manager
        .activate_dev_directory(&package_dir, activation())
        .await
        .expect("activate");
    assert_eq!(health.channel_keys, vec!["local-hello.api".to_string()]);

    let action_result = manager
        .invoke_action("local-hello.profile", json!({ "source": "action" }))
        .await
        .expect("invoke action");
    assert_eq!(action_result["echoed"]["source"], "action");

    let channel_result = manager
        .invoke_channel("local-hello.api", "echo", json!({ "source": "direct" }))
        .await
        .expect("invoke channel");
    assert_eq!(channel_result["echoed"]["source"], "direct");
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn dependency_alias_invokes_provider_channel_in_same_host() {
    let temp = tempfile::tempdir().expect("tempdir");
    let provider_dir = write_provider_package(temp.path()).await.expect("provider");
    let consumer_dir = write_consumer_package(temp.path()).await.expect("consumer");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(
            &provider_dir,
            LocalExtensionHostActivation {
                extension_key: "provider".to_string(),
                ..activation()
            },
        )
        .await
        .expect("activate provider");
    manager
        .activate_dev_directory(
            &consumer_dir,
            LocalExtensionHostActivation {
                extension_key: "consumer".to_string(),
                ..activation()
            },
        )
        .await
        .expect("activate consumer");

    let result = manager
        .invoke_action("consumer.call", json!({ "source": "dependency" }))
        .await
        .expect("invoke consumer");

    assert_eq!(result["echoed"]["source"], "dependency");
    assert_eq!(result["provider"], "provider");
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn channel_method_permissions_guard_host_api_facade() {
    let temp = tempfile::tempdir().expect("tempdir");
    let denied_dir = write_channel_env_package(temp.path(), false)
        .await
        .expect("denied package");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(&denied_dir, activation())
        .await
        .expect("activate denied");

    let err = manager
        .invoke_channel("local-hello.api", "readEnv", Value::Null)
        .await
        .expect_err("permission denied");

    assert!(err.to_string().contains(
        "extension channel method `local-hello.api.readEnv` 未声明 env.read 或 env.read:PATH"
    ));
    manager.stop().await.expect("stop denied manager");

    let allowed_dir = write_channel_env_package(temp.path(), true)
        .await
        .expect("allowed package");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(&allowed_dir, activation())
        .await
        .expect("activate allowed");

    let result = manager
        .invoke_channel("local-hello.api", "readEnv", Value::Null)
        .await
        .expect("invoke allowed");

    assert_eq!(result["has_path"], true);
    manager.stop().await.expect("stop allowed manager");
}

#[tokio::test]
async fn channel_invocation_limits_recursive_calls() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_channel_loop_package(temp.path())
        .await
        .expect("package");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(&package_dir, activation())
        .await
        .expect("activate");

    let err = manager
        .invoke_channel("local-hello.api", "loop", Value::Null)
        .await
        .expect_err("recursive channel");

    assert!(
        err.to_string()
            .contains("extension invocation depth exceeded")
    );
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn runtime_invoke_calls_loaded_extension_action() {
    let temp = tempfile::tempdir().expect("tempdir");
    let provider_dir = write_runtime_provider_package(temp.path())
        .await
        .expect("provider");
    let consumer_dir = write_runtime_consumer_package(temp.path(), true)
        .await
        .expect("consumer");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(
            &provider_dir,
            LocalExtensionHostActivation {
                extension_key: "provider".to_string(),
                ..activation()
            },
        )
        .await
        .expect("activate provider");
    manager
        .activate_dev_directory(
            &consumer_dir,
            LocalExtensionHostActivation {
                extension_key: "consumer".to_string(),
                ..activation()
            },
        )
        .await
        .expect("activate consumer");

    let result = manager
        .invoke_action("consumer.runtime_call", json!({ "source": "runtime" }))
        .await
        .expect("invoke consumer");

    assert_eq!(result["echoed"]["source"], "runtime");
    assert_eq!(result["provider"], "provider");
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn runtime_invoke_requires_cross_extension_permission() {
    let temp = tempfile::tempdir().expect("tempdir");
    let provider_dir = write_runtime_provider_package(temp.path())
        .await
        .expect("provider");
    let consumer_dir = write_runtime_consumer_package(temp.path(), false)
        .await
        .expect("consumer");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(
            &provider_dir,
            LocalExtensionHostActivation {
                extension_key: "provider".to_string(),
                ..activation()
            },
        )
        .await
        .expect("activate provider");
    manager
        .activate_dev_directory(
            &consumer_dir,
            LocalExtensionHostActivation {
                extension_key: "consumer".to_string(),
                ..activation()
            },
        )
        .await
        .expect("activate consumer");

    let err = manager
        .invoke_action("consumer.runtime_call", json!({ "source": "runtime" }))
        .await
        .expect_err("permission denied");

    assert!(err.to_string().contains("runtime.invoke:provider.echo"));
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn runtime_invoke_reports_unloaded_action_without_host_api_fallback() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_package_with_permissions(
        temp.path(),
        unloaded_runtime_invoke_bundle(),
        json!([]),
        json!(["runtime.invoke:provider.missing"]),
    )
    .await
    .expect("package");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(&package_dir, activation())
        .await
        .expect("activate");

    let err = manager
        .invoke_action("local-hello.profile", json!({ "source": "runtime" }))
        .await
        .expect_err("unloaded runtime action");

    assert!(
        err.to_string()
            .contains("runtime action is not loaded in current extension host: provider.missing")
    );
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn runtime_invoke_limits_recursive_calls() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_package(temp.path(), recursive_runtime_bundle(), false, false)
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
        .expect_err("recursive invoke");

    assert!(
        err.to_string()
            .contains("extension invocation depth exceeded")
    );
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn protocol_channel_invoke_reports_unloaded_method_without_host_api_fallback() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_package(temp.path(), unloaded_channel_invoke_bundle(), false, false)
        .await
        .expect("package");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(&package_dir, activation())
        .await
        .expect("activate");

    let err = manager
        .invoke_action("local-hello.profile", json!({ "source": "channel" }))
        .await
        .expect_err("unloaded channel method");

    assert!(err.to_string().contains(
        "extension channel method is not loaded in current extension host: provider.api.echo"
    ));
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
async fn activation_rejects_handlers_not_declared_by_manifest() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_package(temp.path(), extra_action_bundle(), true, true)
        .await
        .expect("package");
    let manager = test_manager(temp.path());

    let err = manager
        .activate_dev_directory(&package_dir, activation())
        .await
        .expect_err("manifest parity");

    assert!(err.to_string().contains(
        "extension action is registered but not declared in manifest: local-hello.extra"
    ));
    let _ = manager.stop().await;
}

#[tokio::test]
async fn activation_rejects_manifest_action_without_handler() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_package(temp.path(), no_action_bundle(), true, true)
        .await
        .expect("package");
    let manager = test_manager(temp.path());

    let err = manager
        .activate_dev_directory(&package_dir, activation())
        .await
        .expect_err("manifest parity");

    assert!(err.to_string().contains(
        "extension action is declared in manifest but not registered: local-hello.profile"
    ));
    let _ = manager.stop().await;
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
async fn top_level_local_profile_summary_does_not_gate_action_call() {
    let temp = tempfile::tempdir().expect("tempdir");
    let package_dir = write_package(temp.path(), profile_bundle(), false, true)
        .await
        .expect("package");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(&package_dir, activation())
        .await
        .expect("activate");
    let result = manager
        .invoke_action("local-hello.profile", Value::Null)
        .await
        .expect("invoke");
    assert_eq!(result["backend_id"], "backend-1");
    manager.stop().await.expect("stop");
}

#[tokio::test]
async fn built_in_host_apis_use_action_permissions_and_workspace_boundary() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("workspace");
    tokio::fs::create_dir_all(&workspace)
        .await
        .expect("workspace");
    let package_dir = write_package_with_permissions(
        temp.path(),
        built_in_host_api_bundle(),
        json!([]),
        json!([
            "workspace.vfs.write",
            "workspace.vfs.read",
            "workspace.vfs.list",
            "env.read:PATH",
            "process.execute"
        ]),
    )
    .await
    .expect("package");
    let manager = test_manager(temp.path());
    manager
        .activate_dev_directory(&package_dir, activation_with_root(workspace))
        .await
        .expect("activate");

    let result = manager
        .invoke_action("local-hello.profile", Value::Null)
        .await
        .expect("invoke");
    assert_eq!(result["file_text"], "hello from extension");
    assert_eq!(result["stat_kind"], "file");
    assert_eq!(result["listed"], true);
    assert_eq!(result["shell"]["exit_code"], 0);
    assert!(
        result["shell"]["stdout"]
            .as_str()
            .unwrap_or_default()
            .contains("host-api-ok")
    );
    assert_eq!(result["has_path"], true);
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
    activation_with_root(PathBuf::from("C:/secret/workspace"))
}

fn activation_with_root(workspace_root: PathBuf) -> LocalExtensionHostActivation {
    LocalExtensionHostActivation {
        extension_key: "local-hello".to_string(),
        backend_id: "backend-1".to_string(),
        project_id: Some("project-1".to_string()),
        session_id: Some("session-1".to_string()),
        default_workspace_root: Some(workspace_root.clone()),
        workspace_roots: vec![workspace_root],
    }
}

async fn write_package(
    root: &Path,
    bundle: String,
    include_top_level_permission: bool,
    include_action_permission: bool,
) -> anyhow::Result<PathBuf> {
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
    write_package_with_permissions(root, bundle, permissions, action_permissions).await
}

async fn write_package_with_permissions(
    root: &Path,
    bundle: String,
    permissions: Value,
    action_permissions: Value,
) -> anyhow::Result<PathBuf> {
    let package_dir = root.join("package");
    tokio::fs::create_dir_all(package_dir.join("dist")).await?;
    write_bundle(&package_dir, bundle.clone()).await?;
    let digest = digest_bytes(bundle.as_bytes());
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

async fn write_channel_echo_package(root: &Path) -> anyhow::Result<PathBuf> {
    let package_dir = root.join("channel-echo");
    tokio::fs::create_dir_all(package_dir.join("dist")).await?;
    let bundle = channel_bundle();
    write_bundle(&package_dir, bundle.clone()).await?;
    let manifest = json!({
        "manifest_version": "2",
        "extension_id": "local-hello",
        "package": { "name": "@agentdash/local-hello", "version": "0.1.0" },
        "asset_version": "0.1.0",
        "runtime_actions": [{
            "action_key": "local-hello.profile",
            "kind": "session_runtime",
            "description": "Read local profile",
            "input_schema": true,
            "output_schema": true,
            "permissions": [],
        }],
        "protocol_channels": [{
            "channel_key": "local-hello.api",
            "version": "1.0.0",
            "description": "Local API",
            "methods": [{
                "name": "echo",
                "description": "Echo input",
                "input_schema": true,
                "output_schema": true,
                "permissions": [],
            }],
        }],
        "bundles": [{
            "kind": "extension_host",
            "entry": "dist/extension.js",
            "digest": digest_bytes(bundle.as_bytes()),
        }],
    });
    tokio::fs::write(
        package_dir.join("agentdash.extension.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .await?;
    Ok(package_dir)
}

async fn write_channel_env_package(
    root: &Path,
    include_method_permission: bool,
) -> anyhow::Result<PathBuf> {
    let package_dir = root.join(if include_method_permission {
        "channel-env-allowed"
    } else {
        "channel-env-denied"
    });
    tokio::fs::create_dir_all(package_dir.join("dist")).await?;
    let bundle = channel_env_bundle();
    write_bundle(&package_dir, bundle.clone()).await?;
    let method_permissions = if include_method_permission {
        json!(["env.read:PATH"])
    } else {
        json!([])
    };
    let manifest = json!({
        "manifest_version": "2",
        "extension_id": "local-hello",
        "package": { "name": "@agentdash/local-hello", "version": "0.1.0" },
        "asset_version": "0.1.0",
        "protocol_channels": [{
            "channel_key": "local-hello.api",
            "version": "1.0.0",
            "description": "Local API",
            "methods": [{
                "name": "readEnv",
                "description": "Read PATH",
                "input_schema": true,
                "output_schema": true,
                "permissions": method_permissions,
            }],
        }],
        "bundles": [{
            "kind": "extension_host",
            "entry": "dist/extension.js",
            "digest": digest_bytes(bundle.as_bytes()),
        }],
    });
    tokio::fs::write(
        package_dir.join("agentdash.extension.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .await?;
    Ok(package_dir)
}

async fn write_channel_loop_package(root: &Path) -> anyhow::Result<PathBuf> {
    let package_dir = root.join("channel-loop");
    tokio::fs::create_dir_all(package_dir.join("dist")).await?;
    let bundle = channel_loop_bundle();
    write_bundle(&package_dir, bundle.clone()).await?;
    let manifest = json!({
        "manifest_version": "2",
        "extension_id": "local-hello",
        "package": { "name": "@agentdash/local-hello", "version": "0.1.0" },
        "asset_version": "0.1.0",
        "protocol_channels": [{
            "channel_key": "local-hello.api",
            "version": "1.0.0",
            "description": "Local API",
            "methods": [{
                "name": "loop",
                "description": "Recursive channel",
                "input_schema": true,
                "output_schema": true,
                "permissions": [],
            }],
        }],
        "bundles": [{
            "kind": "extension_host",
            "entry": "dist/extension.js",
            "digest": digest_bytes(bundle.as_bytes()),
        }],
    });
    tokio::fs::write(
        package_dir.join("agentdash.extension.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .await?;
    Ok(package_dir)
}

async fn write_provider_package(root: &Path) -> anyhow::Result<PathBuf> {
    let package_dir = root.join("provider");
    tokio::fs::create_dir_all(package_dir.join("dist")).await?;
    let bundle = provider_channel_bundle();
    write_bundle(&package_dir, bundle.clone()).await?;
    let manifest = json!({
        "manifest_version": "2",
        "extension_id": "provider",
        "package": { "name": "@agentdash/provider", "version": "1.0.0" },
        "asset_version": "1.0.0",
        "protocol_channels": [{
            "channel_key": "provider.api",
            "version": "1.0.0",
            "description": "Provider API",
            "methods": [{
                "name": "echo",
                "description": "Echo input",
                "input_schema": true,
                "output_schema": true,
                "permissions": [],
            }],
        }],
        "bundles": [{
            "kind": "extension_host",
            "entry": "dist/extension.js",
            "digest": digest_bytes(bundle.as_bytes()),
        }],
    });
    tokio::fs::write(
        package_dir.join("agentdash.extension.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .await?;
    Ok(package_dir)
}

async fn write_consumer_package(root: &Path) -> anyhow::Result<PathBuf> {
    let package_dir = root.join("consumer");
    tokio::fs::create_dir_all(package_dir.join("dist")).await?;
    let bundle = consumer_dependency_bundle();
    write_bundle(&package_dir, bundle.clone()).await?;
    let manifest = json!({
        "manifest_version": "2",
        "extension_id": "consumer",
        "package": { "name": "@agentdash/consumer", "version": "1.0.0" },
        "asset_version": "1.0.0",
        "runtime_actions": [{
            "action_key": "consumer.call",
            "kind": "session_runtime",
            "description": "Call provider",
            "input_schema": true,
            "output_schema": true,
            "permissions": ["extension.channel.invoke:provider.api.echo"],
        }],
        "extension_dependencies": [{
            "alias": "provider",
            "extension_id": "provider",
            "version": "^1.0.0",
            "channels": ["provider.api"],
        }],
        "bundles": [{
            "kind": "extension_host",
            "entry": "dist/extension.js",
            "digest": digest_bytes(bundle.as_bytes()),
        }],
    });
    tokio::fs::write(
        package_dir.join("agentdash.extension.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .await?;
    Ok(package_dir)
}

async fn write_runtime_provider_package(root: &Path) -> anyhow::Result<PathBuf> {
    let package_dir = root.join("runtime-provider");
    tokio::fs::create_dir_all(package_dir.join("dist")).await?;
    let bundle = runtime_provider_bundle();
    write_bundle(&package_dir, bundle.clone()).await?;
    let manifest = json!({
        "manifest_version": "2",
        "extension_id": "provider",
        "package": { "name": "@agentdash/provider", "version": "1.0.0" },
        "asset_version": "1.0.0",
        "runtime_actions": [{
            "action_key": "provider.echo",
            "kind": "session_runtime",
            "description": "Echo input",
            "input_schema": true,
            "output_schema": true,
            "permissions": [],
        }],
        "bundles": [{
            "kind": "extension_host",
            "entry": "dist/extension.js",
            "digest": digest_bytes(bundle.as_bytes()),
        }],
    });
    tokio::fs::write(
        package_dir.join("agentdash.extension.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .await?;
    Ok(package_dir)
}

async fn write_runtime_consumer_package(
    root: &Path,
    include_permission: bool,
) -> anyhow::Result<PathBuf> {
    let package_dir = root.join(if include_permission {
        "runtime-consumer"
    } else {
        "runtime-consumer-denied"
    });
    tokio::fs::create_dir_all(package_dir.join("dist")).await?;
    let bundle = runtime_consumer_bundle();
    write_bundle(&package_dir, bundle.clone()).await?;
    let permissions = if include_permission {
        json!(["runtime.invoke:provider.echo"])
    } else {
        json!([])
    };
    let manifest = json!({
        "manifest_version": "2",
        "extension_id": "consumer",
        "package": { "name": "@agentdash/consumer", "version": "1.0.0" },
        "asset_version": "1.0.0",
        "runtime_actions": [{
            "action_key": "consumer.runtime_call",
            "kind": "session_runtime",
            "description": "Call provider runtime action",
            "input_schema": true,
            "output_schema": true,
            "permissions": permissions,
        }],
        "bundles": [{
            "kind": "extension_host",
            "entry": "dist/extension.js",
            "digest": digest_bytes(bundle.as_bytes()),
        }],
    });
    tokio::fs::write(
        package_dir.join("agentdash.extension.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .await?;
    Ok(package_dir)
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

fn extra_action_bundle() -> String {
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
    ctx.runtime.registerAction({
      action_key: "local-hello.extra",
      kind: "session_runtime",
      description: "Extra",
      invoke() {
        return {};
      },
    });
  },
};
"#
    .to_string()
}

fn no_action_bundle() -> String {
    r#"
export default {
  activate() {},
};
"#
    .to_string()
}

fn recursive_runtime_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: "local-hello.profile",
      kind: "session_runtime",
      description: "Recursive runtime invoke",
      async invoke() {
        return await ctx.api.runtime.invoke("local-hello.profile", {});
      },
    });
  },
};
"#
    .to_string()
}

fn unloaded_runtime_invoke_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: "local-hello.profile",
      kind: "session_runtime",
      description: "Invoke unloaded runtime action",
      async invoke(input) {
        return await ctx.api.runtime.invoke("provider.missing", input);
      },
    });
  },
};
"#
    .to_string()
}

fn unloaded_channel_invoke_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: "local-hello.profile",
      kind: "session_runtime",
      description: "Invoke unloaded channel method",
      async invoke(input) {
        return await ctx.api.channels.invoke("provider.api", "echo", input);
      },
    });
  },
};
"#
    .to_string()
}

fn channel_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.channels.register({
      channel_key: "api",
      version: "1.0.0",
      description: "Echo channel",
      methods: {
        echo: {
          description: "Echo input",
          invoke(input) {
            return { echoed: input };
          },
        },
      },
    });
    ctx.runtime.registerAction({
      action_key: "local-hello.profile",
      kind: "session_runtime",
      description: "Invoke own channel",
      async invoke(input) {
        return await ctx.api.channels.self("api").invoke("echo", input);
      },
    });
  },
};
"#
    .to_string()
}

fn provider_channel_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.channels.register({
      channel_key: "api",
      version: "1.0.0",
      description: "Provider API",
      methods: {
        echo: {
          description: "Echo input",
          invoke(input) {
            return { provider: "provider", echoed: input };
          },
        },
      },
    });
  },
};
"#
    .to_string()
}

fn consumer_dependency_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: "consumer.call",
      kind: "session_runtime",
      description: "Call provider channel",
      async invoke(input) {
        return await ctx.api.channels.from("provider").invoke("echo", input);
      },
    });
  },
};
"#
    .to_string()
}

fn channel_env_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.channels.register({
      channel_key: "api",
      version: "1.0.0",
      description: "Local API",
      methods: {
        readEnv: {
          description: "Read PATH",
          async invoke() {
            const path = await ctx.api.env.get("PATH");
            return { has_path: typeof path === "string" && path.length > 0 };
          },
        },
      },
    });
  },
};
"#
    .to_string()
}

fn channel_loop_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.channels.register({
      channel_key: "api",
      version: "1.0.0",
      description: "Loop channel",
      methods: {
        loop: {
          description: "Loop forever",
          async invoke() {
            return await ctx.api.channels.self("api").invoke("loop", {});
          },
        },
      },
    });
  },
};
"#
    .to_string()
}

fn runtime_provider_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: "provider.echo",
      kind: "session_runtime",
      description: "Echo input",
      invoke(input) {
        return { provider: "provider", echoed: input };
      },
    });
  },
};
"#
    .to_string()
}

fn runtime_consumer_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: "consumer.runtime_call",
      kind: "session_runtime",
      description: "Call provider runtime action",
      async invoke(input) {
        return await ctx.api.runtime.invoke("provider.echo", input);
      },
    });
  },
};
"#
    .to_string()
}

fn built_in_host_api_bundle() -> String {
    r#"
export default {
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: "local-hello.profile",
      kind: "session_runtime",
      description: "Use built-in host APIs",
      permissions: [
        "workspace.vfs.write",
        "workspace.vfs.read",
        "workspace.vfs.list",
        "env.read:PATH",
        "process.execute",
      ],
      async invoke() {
        await ctx.api.workspace.writeText("notes/hello.txt", "hello from extension");
        const fileText = await ctx.api.workspace.readText("notes/hello.txt");
        const entries = await ctx.api.workspace.list("notes");
        const stat = await ctx.api.workspace.stat("notes/hello.txt");
        const pathValue = await ctx.api.env.get("PATH");
        const shell = await ctx.api.process.shell("node -e \"console.log('host-api-ok')\"", {
          timeout_ms: 5000,
          max_output_bytes: 1024,
        });
        return {
          file_text: fileText,
          listed: entries.some((entry) => entry.path === "notes/hello.txt"),
          stat_kind: stat.kind,
          has_path: typeof pathValue === "string" && pathValue.length > 0,
          shell,
        };
      },
    });
  },
};
"#
    .to_string()
}
