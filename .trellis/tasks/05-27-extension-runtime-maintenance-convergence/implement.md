# Extension Runtime 收口维护 Implement Plan

## Step 1: Permission Evaluator

- 定位当前权限规则：
  - `crates/agentdash-domain/src/shared_library/value_objects.rs`
  - `crates/agentdash-application/src/runtime_gateway/extension_actions.rs`
  - `crates/agentdash-local/src/extensions/host/`
- 抽出共享 evaluator 或 domain helper，返回 allow/deny 与 reason metadata。
- 修改 RuntimeGateway extension provider：
  - 对 action invoke 直接使用 evaluator。
  - action 未声明 `local.profile.read` 时，即使 extension 顶层声明 local_profile，也要拒绝。
  - 未知 permission 保持拒绝。
- 修改 local host `local.get_profile`：
  - 使用相同 evaluator/fixture。
  - 错误信息保留 permission key 与 action key，方便诊断。
- 补测试：
  - extension 顶层无 / action 有。
  - extension 顶层有 / action 无。
  - extension 与 action 均有。
  - 未知 permission。

## Step 2: Artifact Storage Port

- 在 application 层定义 `ExtensionPackageArtifactStorage` trait 或等价端口。
- 将 filesystem storage root、storage ref path normalization、object read/write 移到 infrastructure adapter。
- 调整 bootstrap/repository wiring，把 storage adapter 注入 extension package use case 所需上下文。
- 将 upload、archive download、webview asset read 改成调用 application use case。
- 保持 archive digest 校验、manifest validation、bundle digest validation 和 webview asset allowlist 行为不变。
- 检查 Canvas promote 是否继续走同一 archive validation/storage/install 边界。

Progress:

- [x] Storage port 落在 `agentdash-spi::extension_package`，application use case 通过端口消费，避免 infrastructure 反向依赖 application。
- [x] Filesystem archive object adapter 落在 `agentdash-infrastructure::storage`，负责 storage root、storage ref path normalization 与 object read/write。
- [x] Upload、archive download、webview asset read、Canvas promote 已改为经由 application use case + injected storage port。
- [x] `cargo check -p agentdash-api -q` 通过；本次触及文件逐个 `rustfmt --check` 通过。

## Step 3: Local Host Module Split

- 新建 `crates/agentdash-local/src/extensions/host/` 目录。
- 从当前 `extensions/host.rs` 拆出：
  - `manager.rs`
  - `process.rs`
  - `protocol.rs`
  - `permissions.rs`
  - `runner.rs`
- `extensions/mod.rs` re-export 稳定入口。
- 更新 `lib.rs`、handler、runtime 引用，避免调用方依赖内部模块路径。
- 保持行为等价，不在本步骤扩展新 permission 能力。

Progress:

- [x] `extensions/host.rs` 已目录化为 `extensions/host/mod.rs`。
- [x] 拆出 `manager.rs`、`process.rs`、`protocol.rs`、`permissions.rs`、`runner.rs`，外部仍通过 `extensions::host` / crate re-export 消费稳定入口。
- [x] 原 host tests 已迁移到 `extensions/host/tests.rs`，权限行为仍通过同一 domain evaluator。
- [x] `cargo check -p agentdash-local -q` 通过；host 相关文件逐个 `rustfmt --check` 通过。

## Step 4: Frontend / Contract Follow-Up

- 如果 evaluator metadata 或 trust state 进入 DTO：
  - 更新 `agentdash-contracts`。
  - 重新生成 TS contracts。
  - 更新 `packages/app-web/src/services/extensionRuntime.ts` mapper。
  - 补 mapper/bridge 相关测试。
- 如果只是后端内部 metadata，不扩大前端 UI 范围。

Progress:

- [x] 本次 evaluator decision metadata 只进入 runtime invocation output metadata，未改变 contracts DTO 或前端 bridge surface。
- [x] 未引入新的前端 mapper / bridge 变更。

## Validation Commands

按实际改动范围选择执行：

```bash
cargo test -p agentdash-domain extension_template
cargo test -p agentdash-application extension_runtime extension_actions extension_package
cargo test -p agentdash-api extension_runtime extension_package
cargo test -p agentdash-local permission_denied packaged_directory_verifies_bundle_digest
pnpm --filter app-web test -- extensionRuntime bridge extensionTabDescriptors tab-type-registry workspaceTabStore
pnpm --filter @agentdash/extension-dev test
```

如果 `@agentdash/extension-dev` 因本地缺少 `node_modules/esbuild` 报 `ERR_MODULE_NOT_FOUND`，先执行 workspace 依赖安装，再重跑测试；该错误是环境准备问题，不是断言失败。

## Risky Files

- `crates/agentdash-domain/src/shared_library/value_objects.rs`
- `crates/agentdash-application/src/runtime_gateway/extension_actions.rs`
- `crates/agentdash-application/src/extension_package.rs`
- `crates/agentdash-api/src/routes/extension_package_artifacts.rs`
- `crates/agentdash-api/src/routes/extension_runtime.rs`
- `crates/agentdash-api/src/bootstrap/repositories.rs`
- `crates/agentdash-local/src/extensions/host/`
- `crates/agentdash-local/src/extensions/`
- `packages/app-web/src/services/extensionRuntime.ts`

## Rollback Points

- Permission evaluator 可以先在 application/domain 层落地，local host module split 暂缓；两者不应互相阻塞。
- Artifact storage port 如果 bootstrap wiring 过大，可以先完成 use case 抽象，再移动 filesystem adapter。
- Local host 拆分应保持 rename/move 为主，避免同时改变 runner 协议。

## Ready-To-Start Checklist

- [x] 用户确认本任务先收口 trusted local extension 的权限/审计语义，不把 isolated execution 纳入本次实现。
- [x] PRD / design / implement 已 review。
- [x] `task.py start 05-27-extension-runtime-maintenance-convergence` 后再进入实现。
