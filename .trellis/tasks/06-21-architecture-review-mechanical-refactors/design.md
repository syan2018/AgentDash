# 机械化重构任务设计

## Classification Rule

本任务只收纳满足以下条件的 work item：

- 目标文件和模块边界已经清楚。
- 不需要在执行前决定新的事实源归属。
- 不需要引入新的控制面 command 语义。
- 可以通过 contract check、typecheck、focused tests 或静态检索验收。
- 失败时可以局部回滚，不影响其它设计线索。

以下类型属于机械性重构：

- generated contract 覆盖已有手写 DTO。
- 前端 service 从 raw mapper 回到 generated DTO。
- 类型入口拆分、命名收窄、debug/internal surface 守卫。
- 已确认无产品路径的残留 service 删除或移入 test support。
- 补充 focused tests、diagnostics 或 UI 文案，使现有设计更可观察。

以下类型不属于机械性重构：

- 需要决定事实源唯一归属。
- 需要改变 public command 合同。
- 需要定义 runtime coordinate selection policy。
- 需要在多个模块之间选择新的 owner。

## Work Item Groups

### A. Contract Surface

把已经有明确 generated contract 方向的手写 DTO、stream envelope、frontend mapper 收口。主要风险是改动面较宽，但设计方向稳定。

### B. Residual Surface Cleanup

删除或收窄不应作为产品入口的残留 API/service/helper。执行前必须用 `rg` 证明调用面。

### C. Tests / Diagnostics / UI Semantics

补齐测试、可观测诊断和 UI 文案，防止现有设计被误用。

## Execution Policy

- 每次执行优先选择同组内 1-3 个相关 item，避免一次性横跨 backend/frontend/local 多条链路。
- Contract item 改动后先跑 `pnpm run contracts:check`，再跑受影响 package 的 typecheck。
- Cleanup item 先贴出 `rg` 调用证据，再删除/封装。
- UI semantics item 不改变后端事实源。

