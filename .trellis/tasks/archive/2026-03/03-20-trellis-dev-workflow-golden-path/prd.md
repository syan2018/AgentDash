# Trellis Dev Workflow 黄金路径

## Goal

基于前置的 workflow 平台化映射和定义模型，落地 AgentDash 的第一条真实 workflow 主航道：`Trellis Dev Workflow`。

第一版目标不是自动化守护进程，而是跑通一条可人工触发、可阶段推进、可记录输出、可给出归档建议的 workflow run。

## Background

当前项目已经具备：

- `Project Session / Story Session / Task Session`
- `session_plan` 与结构化上下文快照
- `Address Space` 与统一 runtime tools
- `SessionBinding`
- Trellis 的 task / context / journal / archive 约定

因此当前最合理的实现方式不是重写底座，而是把这些能力组合成一条正式 workflow 黄金路径。

## Requirements

- 定义第一版 workflow run 的目标对象范围。
- 让目标对象可以显式启动 `Trellis Dev Workflow`。
- 支持至少四个 phase：
  - `Start`
  - `Implement`
  - `Check`
  - `Record`
- 明确每个 phase 如何绑定现有上下文来源。
- 明确每个 phase 与哪类 session 交互。
- 明确 `Record` 阶段如何生成 summary、journal suggestion、archive suggestion。
- 明确前端至少需要看到哪些 workflow run / phase 信息，避免 run 成为纯后端隐式状态。
- 明确第一版不做自动调度 loop、attempt/retry/control plane。

## Acceptance Criteria

- [ ] 第一版 workflow run 目标对象范围明确。
- [ ] 可显式启动 `Trellis Dev Workflow`。
- [ ] `Start / Implement / Check / Record` 四阶段边界明确。
- [ ] phase 与现有 session/context 注入接线清晰。
- [ ] `Record` 阶段能产出结构化记录建议。
- [ ] 前端至少能看到当前 run 的 phase 与基础状态。

## Out of Scope

- 不做长期后台自动化 loop
- 不做复杂 retry / claim / reconcile 语义
- 不做完整 owner session runtime
- 不做完整 managed workspace lifecycle
- 不做全量 observability 控制台

## Related Files

- `.trellis/tasks/03-20-symphony-flow-long-term-tracking/execution-roadmap.md`
- `.trellis/tasks/03-20-symphony-flow-long-term-tracking/trellis-dev-workflow-golden-path.md`
- `crates/agentdash-application/src/session_plan.rs`
- `crates/agentdash-api/src/routes/project_sessions.rs`
- `crates/agentdash-api/src/routes/story_sessions.rs`
- `crates/agentdash-api/src/routes/task_execution.rs`
- `frontend/src/pages/SessionPage.tsx`
