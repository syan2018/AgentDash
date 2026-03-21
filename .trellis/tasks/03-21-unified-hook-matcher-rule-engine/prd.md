# 统一 Hook matcher 与 rule engine

## Goal

把当前以 workflow phase 为主的 hook policy 生成逻辑收敛成统一 matcher/rule engine，让 workflow、task、story、project、global builtin hook 共用同一套执行模型。

## Background

当前 Hook Runtime 已经具备：

- `HookPolicy` 结构
- workflow phase -> policy 的第一版映射
- `BeforeTool` / `BeforeStop` 等节点对 policy 的消费

但这仍然是“provider 中的业务 if/else + runtime 少量硬编码消费”。如果不继续收敛：

- workflow 仍会演化成隐式 hook engine
- task/story/global hook 很难接入同一执行面
- policy 行为会散在 provider 和 delegate 两侧

## Scope

- 定义 rule 的统一匹配维度
- 设计 source provider -> normalized rule -> runtime resolution 的链路
- 收敛 workflow phase policy 的数据模型
- 为 future builtin hook / owner hook / companion hook 提供统一承载层

## Requirements

- workflow 只负责声明数据，不再负责生命周期逻辑
- matcher 至少支持：trigger / owner / workflow_phase / tool / task_status / tags
- 运行态 resolution 结构必须稳定，便于前端展示与调试
- policy 合成结果必须可解释来源

## Acceptance Criteria

- [ ] 明确 normalized hook rule 结构
- [ ] 明确 provider 组合与 matcher 的职责边界
- [ ] workflow phase policy 能迁移到统一 matcher 模型
- [ ] 预留 project/story/task/global builtin hook 的接入方式
- [ ] 给出从当前实现平滑迁移的实施路径

## References

- [execution_hooks.rs](/F:/Projects/AgentDash/crates/agentdash-api/src/execution_hooks.rs)
- [hooks.rs](/F:/Projects/AgentDash/crates/agentdash-executor/src/hooks.rs)
- [workflow_runtime.rs](/F:/Projects/AgentDash/crates/agentdash-api/src/workflow_runtime.rs)
- [trellis_dev_task.json](/F:/Projects/AgentDash/crates/agentdash-application/src/workflow/builtins/trellis_dev_task.json)
