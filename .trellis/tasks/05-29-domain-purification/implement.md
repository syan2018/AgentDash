# Domain 净化执行计划

## 预检

- [x] 读取 PRD 与 wave2 checklist。
- [x] 确认 DDD 方向：domain 不依赖 contracts/protocol DTO。
- [x] 复核 `contracts::workflow` re-export domain 类型。
- [x] 复核 session id alias 使用面。

## 实施顺序

1. Workflow contract wire DTO
   - [x] 在 `agentdash-contracts/src/workflow.rs` 定义 workflow wire DTO。
   - [x] 保持 serde shape 与当前 domain JSON 一致。
   - [x] `generate_ts.rs` 继续只导出 contract DTO，`contracts::workflow` 不再 re-export domain。
   - [x] MCP 输入 schema 改为协议层 JSON payload 边界，再解析为 domain 类型。
   - [x] 运行 `pnpm run contracts:check`。

2. 移除 domain TS/Schema derive
   - [x] 删除 domain `ts-rs` / `schemars` 依赖。
   - [x] 清理 domain 中 `TS` / `JsonSchema` derive 与 imports。
   - [x] `cargo check --workspace`。

3. Session id 假 alias
   - [x] 删除 `session_binding/session_id.rs` 中 alias。
   - [x] 将 domain 字段改回 `String`，保留字段名语义。
   - [x] 更新相关注释和 re-export。
   - [x] `rg "pub type (SessionId|StorySessionId|ChildSessionId) = String" crates/agentdash-domain`。

4. 验证与收尾
   - [x] `pnpm run contracts:check`。
   - [x] `cargo check --workspace`。
   - [x] `cargo test -p agentdash-domain --lib`。
   - [x] `cargo test -p agentdash-mcp --lib`。
   - [x] `pnpm -C packages/app-web exec tsc --noEmit`。
   - [x] 更新 PRD、progress checklist、journal 与 spec。
   - [ ] 提交并归档。

## 风险点

- Workflow wire DTO 必须保持现有 serde tag/default，避免 TS 生成看似通过但 API JSON shape 漂移。
- `ToolCapabilityPath` 在 domain 有 parse/reduction 行为，contract 侧只表达 string wire，不复制行为。
- session id newtype 推迟不是兼容方案，而是避免引入没有不变量收益的样板类型；如后续需要编译期隔离，应单独做 repository/API/persistence 全链路 newtype。
