# Extension Artifact Storage 边界抽离 Implement Plan

## Step 1: Inspect

- `crates/agentdash-api/src/routes/extension_package_artifacts.rs`
- `crates/agentdash-api/src/routes/extension_runtime.rs`
- `crates/agentdash-application/src/extension_package.rs`
- `crates/agentdash-api/src/app_state.rs` 如需要装配 service。

## Step 2: Move Storage Helper

- 将 route-local `storage_root`、`storage_object_path`、`write_storage_object`、`read_storage_object` 移到 application extension package 模块，按需要改为 public helper 或 service。
- 保留原有安全路径检查语义。

## Step 3: Update Routes

- package artifact route 改为 import application storage helper。
- extension runtime route 改为 import application storage helper。
- 删除 route-to-route import。

## Step 4: Verify

```powershell
cargo check -p agentdash-api
cargo test -p agentdash-api extension
```
