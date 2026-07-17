# W6 — AgentRun、API、App Server Protocol 与 Frontend Cutover

## Depends On

- W4 authoritative snapshot/change/fork。
- W5 context/compaction/continuation状态与Agent Change冻结。

## Goal

把所有产品消费者切到Hosted Agent boundary，使AgentRun/API、Codex App Server notification与frontend从同一个Agent snapshot/change得出状态，并删除timing、journal、worker和item-type推断。

## Scope

- AgentRun execute/read/changes cutover；
- API operation receipt/availability；
- App Server protocol projection；
- contextCompaction item/Turn lifecycle；
- frontend snapshot+tail reducer；
- derived activity与queued/failed/lost展示；
- cursor gap snapshot recovery；
- generated contracts与旧字段删除。

## Ownership

主要负责：

- `crates/agentdash-application-agentrun/**`
- `crates/agentdash-api/**` 的Agent/session routes与stream
- protocol projector/wire consumer
- `packages/app-web/src/features/session/**` 及相关generated types
- contract generation outputs

开始前必须检查工作区并声明frontend文件ownership，避免覆盖并行会话。

## Deliverables

- AgentRun facade hard cut；
- API receipt/stream；
- Codex-shaped notification mapper；
- frontend authoritative reducer；
- compaction card真实lifecycle；
- generated contract同步。

## Acceptance Criteria

- [ ] AgentRun Session/feed/fork/context/terminal不读取Runtime journal。
- [ ] API不再返回基于调用时机推断的`scheduled_next_turn`/`launched_compaction_turn`。
- [ ] queued compaction无fake `turn/started`/`item/started`。
- [ ] active compaction顺序为turn started、item started、item completed/error、turn completed。
- [ ] notification只在Agent commit后发布且重复投递幂等。
- [ ] frontend activity只从active Turn kind与Session consistency派生。
- [ ] contextCompaction card表达started/succeeded/failed/cancelled/lost。
- [ ] cursor gap触发snapshot reread。
- [ ] manual B后无C；automatic C只在独立promotion change后出现。

## Non-Goals

- 不在frontend复刻Agent transition engine。
- 不改变upstream Codex item payload来塞平台status。
- 不保留旧API字段或reducer fallback。

## Validation

```powershell
cargo test -p agentdash-application-agentrun
cargo test -p agentdash-api agent_runtime
pnpm contracts:generate
pnpm contracts:check
pnpm --filter app-web typecheck
pnpm --filter app-web test -- sessionStreamReducer
pnpm --filter app-web test -- useSessionFeed
pnpm --filter app-web test -- SessionChatViewParts
```
