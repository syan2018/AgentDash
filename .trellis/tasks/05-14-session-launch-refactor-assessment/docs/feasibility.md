# Feasibility

## 结论

目标可实现，但当前分支尚未完成。现有代码已经提供迁移基础：

- `UserPromptInput` 已经能表达纯用户输入。
- `LaunchCommand` 已经成为主要入口。
- `SessionConstructionPlan` / `SessionConstructionPlanner` 已经存在。
- `LaunchExecution` / `SessionLaunchPlanner` / `SessionLaunchExecutor` 已经存在。
- runtime registry、turn supervisor、terminal effect outbox、runtime command store 已经有基础实现。

真正的剩余难点不是“再抽几个类型”，而是继续删除过渡 envelope 和 facade 所承载的隐式字段所有权。

## 可行收口路径

1. 保持 `LaunchCommand` 纯入口意图，并保持 `PromptAugmentInput` 归零。
2. 将 API/bootstrap/assembler 当前写入 `SessionConstructionFacts` 的增强结果迁入 `SessionConstructionPlan` / `LaunchExecution`，不再返回 seed。
3. 补全 `SessionConstructionPlan` 的 context bundle/frame、MCP、capability、identity、trace 字段。
4. 将 `SessionLaunchPlanner` 输入改为 `LaunchCommand + SessionConstructionPlan + runtime facts`。
5. 将 connector input 作为 `LaunchExecution` 内部字段投影为 `ExecutionContext`。
6. 清理 `prompt_pipeline` 中剩余 planning/fallback 职责。
7. 将 `SessionHub` 业务方法拆到能力服务，Hub 删除或仅保留无业务转发。
8. 补齐 effects / pending / persistence 的最终验证。

## 风险

- API bootstrap 目前依赖 repos/AppState，直接生成完整 construction 会触碰 application/API 分层。需要把依赖方向设计清楚，避免把 AppState 继续藏进 construction provider。
- assembler 仍把多类 composition 结果写回 `SessionConstructionFacts`。删除 seed 时要一次性迁移 task/story/project/companion/lifecycle 几条路径，否则会出现新旧两套主线。
- `SessionConstructionPlan` 当前 context 字段过薄。若不先补全字段，`LaunchExecution` 仍会从别处读 bundle/frame。
- `SessionHub` 拆除会触碰测试和大量 helper。拆之前要先确保 launch/runtime/effects/pending 服务有清晰依赖边界。

## 不可接受方案

- 将 `SessionConstructionFacts` 当成最终边界继续传递。
- 新增只转发旧 payload 的 launch service。
- 让 route/context query 与 launch 各自构造 VFS/capability/context，再用测试维持一致。
- 用 wrapper 解释有业务判断的 `SessionHub`。
- 在 terminal effect 上依赖内存 callback 成功来掩盖 replay 不可用。
