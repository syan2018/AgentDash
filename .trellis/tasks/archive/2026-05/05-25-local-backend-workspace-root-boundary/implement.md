# 本机后端 workspace root 授权边界重构执行计划

## Checklist

- [x] 读取相关 spec：backend architecture、runtime gateway、desktop local runtime、project backend workspace routing、cross-layer thinking guide。
- [x] 梳理 `accessible_roots` 所有读写点，分类为 status 投影、目录选择、workspace detect、执行边界、runtime home / SQLite / MCP config。
- [x] 在 local runtime 中拆出 setup path 校验：`workspace.detect` / `detect_git` 使用目录存在性和可读性校验，不复用 `ToolExecutor::validate_workspace_root` 的执行边界校验。
- [x] 调整 `ToolExecutor` 命名和错误语义：表达 `mount_root_ref` / workspace root 边界，避免对用户暴露旧 `accessible_roots` 概念。
- [x] 明确空 workspace roots 语义：不阻断 browse、detect、register；执行时以 `mount_root_ref` 自身为边界。
- [x] 处理显式 workspace roots 非空时的执行校验，确保 canonicalize 后路径比较稳定，配置错误能给出明确原因。
- [x] 将 `accessible_roots` 产品概念迁移为 `workspace_roots` / `mount_root_ref` 边界语义；需要数据库字段调整时补充 migration。
- [x] 调整 ProjectBackendAccess inventory register / refresh 与 runtime health roots 的关系，避免 refresh 把空 roots 当作异常；必要时改成只刷新已知 inventory 或显式 roots。
- [x] 检查 Tauri profile、Local Runtime UI、Settings / workspace candidate 展示，移除空 roots 的错误感或旧白名单心智。
- [x] 更新 relay protocol 注释、cross-layer spec 和必要 docs。
- [x] 增加或调整 Rust 测试：detect 允许未预声明目录、工具执行仍阻止逃逸、空 roots 不阻断、非空 roots 拒绝越界。
- [x] 增加或调整前端测试：目录选择/登记错误展示不再提旧 `accessible_roots`。
- [x] 运行验证命令并记录结果。

## Candidate Files

- `crates/agentdash-local/src/tool_executor.rs`
- `crates/agentdash-local/src/handlers/workspace.rs`
- `crates/agentdash-local/src/handlers/prompt.rs`
- `crates/agentdash-local/src/runtime.rs`
- `crates/agentdash-local/src/ws_client.rs`
- `crates/agentdash-relay/src/protocol/workspace.rs`
- `crates/agentdash-relay/src/protocol/tool.rs`
- `crates/agentdash-api/src/routes/backend_access.rs`
- `crates/agentdash-api/src/workspace_resolution.rs`
- `crates/agentdash-application/src/runtime_gateway/setup_actions.rs`
- `crates/agentdash-application/src/workspace/detection.rs`
- `crates/agentdash-application/src/workspace/resolution.rs`
- `packages/views/src/local-runtime/LocalRuntimeView.tsx`
- `packages/app-web/src/pages/SettingsPage.tsx`
- `packages/app-web/src/components/layout/workspace-layout.tsx`
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`
- `.trellis/spec/cross-layer/desktop-local-runtime.md`

## Validation Commands

```powershell
cargo test -p agentdash-local
cargo test -p agentdash-application runtime_gateway
cargo test -p agentdash-application workspace
cargo test -p agentdash-api workspace
cargo test -p agentdash-api backend_access
pnpm test -- --run
pnpm typecheck
```

If package-level test filters differ from the current workspace scripts, use the nearest existing cargo / pnpm commands and record the exact commands run.

## Validation Results

- `cargo fmt`：通过。
- `pnpm install --frozen-lockfile`：通过；本地 `node_modules` 缺失，安装后前端检查可运行。
- `pnpm run shared:check`：通过。
- `pnpm run frontend:check`：通过。
- `node --check scripts/dev-joint.js`：通过。
- `cargo check -p agentdash-domain`：通过。
- `cargo check -p agentdash-relay`：通过。
- `git diff --check`：通过。
- `cargo test -p agentdash-local workspace_root --lib`：未跑到本 crate 测试，当前依赖 `starlark_map v0.13.0` 在编译阶段因 `Allocative` / 多版本 `hashbrown` 冲突失败。
- `cargo check -p agentdash-api`：同样被 `starlark_map v0.13.0` 编译错误阻断。

## Review Gates

- Confirm `workspace.detect` cannot write files and only reads metadata needed for workspace identity.
- Confirm all execution paths still resolve relative tool input under `mount_root_ref`.
- Confirm empty workspace roots do not produce warnings/errors in normal browse/register flows.
- Confirm no user-facing `accessible_roots` concept remains; any remaining code reference must be an intentional internal transition point with a clear follow-up in the same task.
- Confirm spec language explains why browse, detect/register and execute have different safety boundaries.

## Rollback Points

- If session runtime home / SQLite root cannot be fully decoupled safely in this task, isolate it behind a named helper and document the follow-up, while still removing it from detect/register authorization decisions.
