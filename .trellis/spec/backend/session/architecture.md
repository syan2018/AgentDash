# Agent Runtime Conversation Architecture

## 1. Scope / Trigger

本规范定义 AgentRun 产品坐标如何映射到 Managed Agent Runtime conversation。新增消息、steer、interrupt、interaction、context read/compact、fork/resume 或 runtime trace 功能时复核。产品 Lifecycle/AgentFrame 仍拥有业务归属与期望 surface；Runtime 独占执行会话事实。

## 2. Signatures

```rust
#[async_trait]
pub trait AgentRunRuntime: Send + Sync {
    async fn inspect(&self, target: AgentRunRuntimeTarget) -> Result<AgentRunRuntimeView, Error>;
    async fn send_message(&self, command: SendAgentRunMessage) -> Result<RuntimeCommandReceipt, Error>;
    async fn compact_context(&self, command: GuardedAgentRunCommand) -> Result<RuntimeCommandReceipt, Error>;
    async fn steer_turn(&self, command: SteerAgentRunTurn) -> Result<RuntimeCommandReceipt, Error>;
    async fn interrupt(&self, command: GuardedAgentRunCommand) -> Result<RuntimeCommandReceipt, Error>;
    async fn resolve_interaction(&self, command: ResolveAgentRunInteraction) -> Result<RuntimeCommandReceipt, Error>;
}
```

```text
AgentRun product command
  -> durable AgentRun mailbox/client command id
  -> AgentRunRuntime facade
  -> Runtime binding/provisioning
  -> canonical Runtime operation + outbox
  -> Integration Driver Host
  -> Driver event
  -> canonical snapshot/event cursor
```

## 3. Contracts

- `AgentRunRuntime` facade 只做 product coordinate、authorization/admission input 与 canonical Runtime command 的映射，不保存 Thread/Turn/Item/Interaction 状态。
- `agent_run_runtime_binding` 是 `run_id + agent_id` 到 Runtime thread/Host binding 的唯一产品锚点。Host binding 与 Managed Runtime binding 由 Host activation 原子创建；产品锚点不复制 driver/source coordinate authority。
- mailbox 保存 canonical accepted Runtime operation ID。client command 重试返回同一 receipt，不产生第二 outbox side effect。
- Managed Runtime journal、snapshot、context head、HookRun/effect、tool call 与 durable cursor 是执行会话唯一事实源。
- AgentFrame 与 Business Surface 提供产品期望；`RuntimeOffer` 提供 service 实际保证；admission 持久化 `BoundAgentSurface`。required contribution 未应用时 dispatch 不可用。
- command availability 来自 canonical Runtime snapshot/profile。Lifecycle status、AgentFrame status、Backbone 或 transcript 只用于产品展示，不能制造执行权限。
- compaction 使用 candidate preparation、driver activation、active-head CAS 与 recovery saga；opaque context 不得进入平台 active head。
- disconnect 对 active binding exactly-once 收敛为 `BindingLost`，并 terminalize active Thread/Turn/Operation 为 `Lost`；旧 generation 晚到事件被 fence。

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| AgentRun 无 durable Runtime binding | provision through Integration offer or return typed unavailable |
| duplicate client command | replay original operation receipt; no duplicate dispatch |
| expected revision/active turn 不匹配 | typed stale rejection before side effect |
| command 不在 availability 中 | typed unsupported/unavailable before outbox |
| required surface revision 未应用 | dispatch unavailable |
| Driver event 生命周期非法 | quarantine + critical Lost convergence |
| stale generation event | fence without cursor advance |
| context activation crash | recovery resumes same compaction operation |

## 5. Good / Base / Bad Cases

- Good：用户消息进入 mailbox，facade provision/复用 binding，Managed Runtime 原子接受 operation/outbox，Driver 完成后 UI 从 snapshot/events 观察同一事实。
- Base：重复提交同一 `client_command_id` 返回原 operation receipt；worker claim 到期后由新 lease token 接管。
- Bad：Application 自己维护 active turn，或从 product status 判断可 interrupt，再直接调用具体 Codex/Native client。

## 6. Tests Required

- Facade 与 mailbox tests：coordinate mapping、idempotency、stale guard、availability、operation receipt。
- PostgreSQL tests：binding、operation/event sequence、outbox/worker lease、context/hook/tool exactly-once。
- Native/Codex production composition tests 与 enterprise remote RuntimeWire E2E。
- API/frontend tests：runtime snapshot/events/context endpoints 与 snapshot-only command availability。
- Migration test：旧 session/delivery tables、columns 与 production readers 全部不存在。

## 7. Wrong vs Correct

```rust
// Wrong
if lifecycle_agent.status.is_running() { connector.cancel(session_id).await?; }

// Correct
let view = agent_run_runtime.inspect(target.clone()).await?;
view.require_available(RuntimeCommandKind::Interrupt)?;
agent_run_runtime.interrupt(command.guarded_by(&view)).await?;
```
