# AgentRun Product Hook Orchestration

## 1. Scope / Trigger

本规范适用于 AgentRun 采用 Product Hook definition、处理 Complete Agent callback、持久化
HookRun/HookEffect，以及把 Agent-visible continuation 交给 Mailbox。修改 Hook outcome、
AfterTurn/BeforeStop boundary、Product evaluator、surface requirement 或 Hook durable identity
时必须复核本规范。

## 2. Signatures

```rust
pub struct AgentHookOutcome {
    pub decisions: Vec<AgentHookDecision>,
    pub refresh_surface: bool,
    pub continue_turn: Vec<AgentInputContent>,
    pub diagnostics: Vec<String>,
}

#[async_trait]
pub trait CompleteAgentHookHandler {
    async fn invoke(
        &self,
        callback: ResolvedCompleteAgentHookCallback,
    ) -> Result<AgentHookOutcome, AgentHostCallbackError>;
}
```

Complete Agent Host 解析当前 bound surface、校验 callback route/deadline/allowed actions，再把
immutable callback 交给 Product handler。AgentRun Product 从 committed binding 定位精确
AgentFrame Hook definition并执行；AgentRuntime只承载 callback transport。

## 3. Contracts

- AgentRun Product 以 `run_id + agent_id` 为 Hook workflow owner。HookRun durable identity 同时
  包含 callback idempotency key、definition、trigger、runtime thread、source turn/item/
  interaction 与 binding generation。
- Complete Agent surface 声明 mechanism fidelity；Product definition 决定业务 gate、rewrite、
  context、effect、refresh 与 continuation。Host逐项验证 composite outcome，不能因同次
  resolution含多个合法 action而丢弃其中一个。
- BeforeTool/AfterTool在 tool safe boundary执行同步 gate/rewrite/context；AfterTurn/BeforeStop
  在 Agent loop safe boundary执行。BeforeStop continuation让当前 loop继续，AfterTurn
  continuation先于 BeforeStop gate消费。
- 所有 Agent-visible continuation 先以稳定 HookRun source identity写入 AgentRun Mailbox，再返回
  `continue_turn`。AfterTurn使用 `AgentLoopTurnBoundary`，BeforeStop使用
  `AgentRunTurnBoundary + ContinueOnStop`，两者均使用 `DrainMode::All`。
- HookRun 按 `accepted -> running -> succeeded | failed`保存。成功 outcome及其 effect set
  digest可幂等重放；相同 callback identity指向不同 owner/definition/correlation时返回 conflict。
- continuation对应的 HookEffect只有在 Mailbox message durable materialize后才可记为 applied；
  effect行保存 payload digest、idempotency key和 mailbox message ref，使审计可证明 Product
  delivery事实。
- Dash concrete source上的 callback、Steer与 surface mutation通过同一 owner writer串行提交；
  repository CAS继续作为外部 stale fence，而不是同 owner协程之间的互斥工具。
- Product Hook plan/HookRun/HookEffect 使用 `agent_run_*` schema。其 runtime thread/source/turn
  字段是 concrete Agent evidence，不改变 Product owner。

## 4. Validation & Error Matrix

| 场景 | 必须得到的结果 |
| --- | --- |
| callback route、generation或deadline过期 | side effect前返回 typed stale/unavailable |
| definition或action不在 bound surface | typed unsupported；不执行 Product definition |
| HookRun identity exact replay且已成功 | 返回 durable composite outcome，不重复 Mailbox message |
| HookRun identity与 owner/correlation冲突 | typed conflict；不覆盖 durable facts |
| Product evaluator返回多个合法 action | 按原顺序保留 decisions，并独立保留 refresh/continue/diagnostics |
| AfterTurn/BeforeStop产生 continuation | 先写 Mailbox，再返回 Agent loop可消费内容 |
| BeforeStop没有 continuation或deny | Agent loop自然停止 |
| Dash callback与显式Steer并发 | owner writer串行提交两次合法 mutation |

## 5. Good / Base / Bad Cases

- Good：AfterTurn rule同时产生 context、effect与 continuation；Product先保存同一个 HookRun和
  Mailbox message，Host校验 composite outcome，Native loop在安全边界继续。
- Base：规则只返回 Allow；HookRun保存空 effect set并幂等返回。
- Bad：callback直接向 concrete Agent硬 Steer，或通用 AgentRuntime保存 Product Hook policy；
  这会绕过 Mailbox provenance并形成第二个 workflow owner。

## 6. Tests Required

- Product handler测试覆盖 exact replay、identity conflict、composite resolution、
  AfterTurn/BeforeStop Mailbox materialization。
- Host测试覆盖 outcome内每一种 decision/action以及 refresh/continue的 surface gate。
- Native测试覆盖 AfterTurn continuation先于 BeforeStop、BeforeStop gate继续/停止和 callback
  error terminal。
- Dash测试覆盖 callback commit与Steer并发，共享 owner writer且保留 external CAS fence。
- PostgreSQL migration/readiness测试覆盖 `agent_run_hook_runs/effects` 与 Mailbox外键。
- contracts generation/check覆盖 `AgentHookOutcome` 与 ContinueTurn/RefreshSurface profile。

## 7. Correct Flow

```text
Native safe boundary
  -> Complete Agent Host validates bound callback
  -> AgentRun Product evaluates exact Hook definition
  -> HookRun accepted/running
  -> Agent-visible continuation materializes Mailbox message
  -> HookRun outcome/effects commit
  -> Host validates composite outcome
  -> Native loop consumes continuation
```
