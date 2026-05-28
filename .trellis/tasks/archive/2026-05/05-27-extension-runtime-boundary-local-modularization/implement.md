# Extension Runtime 边界收口与 local 模块目录化 Implement Plan

## Before Starting

- 阅读相关 spec：
  - `.trellis/spec/backend/architecture.md`
  - `.trellis/spec/backend/runtime-gateway.md`
  - `.trellis/spec/backend/directory-structure.md`
  - `.trellis/spec/cross-layer/desktop-local-runtime.md`
  - `.trellis/spec/cross-layer/frontend-backend-contracts.md`
  - `.trellis/spec/cross-layer/shared-library-contract.md`
  - `.trellis/spec/frontend/directory-structure.md`
  - `.trellis/spec/guides/cross-layer-thinking-guide.md`
- 先确认 open decision：TS Extension Host 首版按 trusted 明示，还是直接实现 isolated execution。
- 不启动实现，直到任务从 planning 进入 active 并完成范围确认。

## Phase 1: Trust Boundary

- 定位 `crates/agentdash-local/src/extension_host.rs` 中 runner 字符串、Node `vm` context 初始化与 host API 注入点。
- 根据 open decision 更新 host contract：
  - trusted：调整命名、文档和 UI/contract 表达，移除 sandbox 安全承诺。
  - isolated：拆出 runner/protocol，改为独立隔离执行单元和显式 IPC。
- 增加验证当前 escape case 的测试或 fixture：`setTimeout.constructor("return process")()` 不应在 isolated 模型下获得 Node process。

## Phase 2: Permission Evaluator

- 梳理 manifest 顶层 permissions、runtime action permissions、RuntimeGateway dynamic provider、local host `allows_local_profile` 的现有数据结构。
- 新增共享 evaluator 或共享 fixture，使 Gateway admission、runtime projection 和 local host enforcement 使用同一规则。
- 增加至少以下测试：
  - 顶层 `local_profile` 有权限但 action permissions 为空时的预期行为。
  - 顶层无权限但 action 声明 `local.profile.read` 时的预期行为。
  - 两者都声明时允许读取 profile。
  - 未知权限默认拒绝或不授予 host API。

## Phase 3: Artifact Storage Service

- 从 `agentdash-api/src/routes/extension_package_artifacts.rs` 中识别 `write_storage_object`、`read_storage_object`、`storage_root` 等 storage helper。
- 在 application/infrastructure 边界引入 artifact storage service 或 port。
- 改造 package upload/install、runtime webview asset read、local archive download route，使它们通过同一 storage service。
- 删除 route-to-route storage helper import，API route 只保留 DTO 和错误映射。

## Phase 4: Local Module Directory

- 先移动 extension 相关文件，不重排整个 crate。
- 候选迁移：
  - `extension_artifact_cache.rs` -> `extensions/artifact_cache.rs` 或 `extensions/artifacts.rs`
  - `extension_host.rs` -> `extensions/host/mod.rs`，必要时拆出 `runner.rs`、`protocol.rs`、`permissions.rs`
  - `handlers/extension.rs` -> `handlers/extension/mod.rs`，按 invoke/artifact/cache 准备逻辑拆分
- 更新 `crates/agentdash-local/src/lib.rs` re-export，保持 crate 外稳定入口。
- 跑 `cargo fmt` 和 local crate check，确保纯移动不会引入行为漂移。

## Phase 5: Frontend And Contract Follow-Up

- 如果 trust/permission contract 影响 DTO，更新 `agentdash-contracts` 与生成文件。
- 如果 UI 需要展示 trusted extension 状态，只在 extension 安装/运行入口显示真实状态，不做泛化说明页。
- 复测 `packages/app-web` 中 extension runtime、bridge、tab registry、workspace tab store 相关测试。

## Validation Commands

```powershell
cargo test -p agentdash-local extension_host
cargo test -p agentdash-application runtime_gateway
cargo test -p agentdash-api extension
cargo check -p agentdash-local
pnpm run contracts:check
pnpm --filter app-web test -- extensionRuntime bridge extensionTabDescriptors tab-type-registry workspaceTabStore
pnpm --filter @agentdash/extension-dev test
```

如果 `@agentdash/extension-dev` 因本地缺少 `node_modules/esbuild` 报 `ERR_MODULE_NOT_FOUND`，先执行 workspace 依赖安装，再重跑测试；该错误应作为环境准备问题记录，不算测试断言失败。

## Risky Files

- `crates/agentdash-local/src/extension_host.rs`
- `crates/agentdash-local/src/extension_artifact_cache.rs`
- `crates/agentdash-local/src/handlers/extension.rs`
- `crates/agentdash-application/src/runtime_gateway/extension_actions.rs`
- `crates/agentdash-api/src/routes/extension_package_artifacts.rs`
- `crates/agentdash-api/src/routes/extension_runtime.rs`
- `crates/agentdash-contracts/src/extension_runtime.rs`
- `packages/app-web/src/features/extension-runtime/`
- `packages/extension-sdk/`
- `packages/extension-ui/`
- `packages/extension-dev/`
