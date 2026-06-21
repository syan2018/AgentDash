# Control Surface 执行计划

## Phase 1: Command Contract Design

- [ ] 固化 command taxonomy。
- [ ] 定义 lifecycle create / continue command contract。
- [x] 定义 command availability core resolver 与 ConversationSnapshot 的关系。

## Phase 2: Independent Refactors

- [x] Lifecycle start API 拆分 create Ready run 与 continue/drain command。
- [ ] Hook mailbox NotFound fallback 收口。
- [x] Extension backend target resolver 统一。
- [x] Command availability core resolver 接入 ConversationSnapshot 与 workspace route policy。
  - ProjectAgent draft start 归前端本地 draft action 与 ProjectAgent 创建入口；已 materialized AgentRun 的 backend availability 只发出 runtime workspace command。
- [ ] Extension channel admission parity。

## Phase 3: Placement Boundaries

- [ ] Relay command target taxonomy 写入 cross-layer spec。
- [ ] Terminal 与 execution lease 的产品语义落定并更新 UI/runtime-summary。

## Validation

```powershell
cargo test -p agentdash-api lifecycle
cargo test -p agentdash-application workflow
cargo test -p agentdash-application agent_run
pnpm run frontend:check
```
