# Fix 023: companion tool runtime context

## 范围

- `crates/agentdash-application/src/companion/tool_context.rs`
- `crates/agentdash-application/src/companion/tools.rs`
- `crates/agentdash-application/src/companion/mod.rs`
- `crates/agentdash-application/src/companion/gate_control.rs`
- `crates/agentdash-application/src/vfs/tools/provider.rs`

## 更新

- 新增 `CompanionToolContext`，集中保存 delivery runtime session id、turn id、hook runtime，以及解析后的 lifecycle anchor。
- provider 在构造 companion tools 时统一异步解析 context；anchor 缺失、agent/frame 不存在或 run/frame 不一致时保留具体错误，执行需要 anchor 的动作时直接返回该错误。
- 新增 `require_session_services(action)`，companion request/respond 生产路径在 session services 未初始化时 fail closed。
- respond parent gate / child result 回流不再使用 `NoopCompanionGateDelivery`；noop delivery 仅保留在 gate control 测试编译范围。
- 新增 `CompanionGateControlFactory`，统一从 repos + `SessionEventingService` 构造 `CompanionGateControlService`。
- 新增 `CompanionHookProvenance` helper，集中生成 companion hook evaluate/refresh provenance source。

## 验证

- `cargo check -p agentdash-application`：通过。
- `cargo test -p agentdash-application companion`：通过，38 passed。
- `cargo test -p agentdash-application vfs::tools::provider`：通过，0 个匹配测试。

## 备注

- 未实现 child session launch，未接入 platform grant。
- 测试输出存在既有 `session::construction` dead_code warnings，本轮未触及。
