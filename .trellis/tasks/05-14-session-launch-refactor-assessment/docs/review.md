# Reference Review Alignment

## 结论

`docs/reviews/AgentDash_session_refactor_plan.md` 的目标与当前目标一致的部分是：删除多入口半成品请求链路、消除隐式 fallback、统一 owner/context/能力面、拆掉 `SessionHub` 的跨域职责、让 terminal 副作用事件化。

当前目标与参考 review 的主要背离是：参考 review 把核心抽象命名并组织为 `LaunchCommand -> LaunchPlanner -> LaunchPlan -> LaunchExecutor`；当前目标把它拆成更明确的数据边界：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution
```

这个背离不是降低目标，而是把 review 里的 `LaunchPlan` 拆成一个稳定事实源和一个短生命周期执行计划，避免把 `PromptSessionRequest` 换名成新的大对象，也避免把 resolver/planner/projector 固化为无意义的传递层。

## 对齐点

- 所有来源都进入唯一 launch 数据流，不再在 HTTP / Task / Workflow / Routine / Companion / Local relay 各自组装最终请求。
- `PromptSessionRequest` 从生产主链路删除。
- `SessionHub` 不作为最终业务 facade 保留。
- owner 解析收敛为单一 `ResolvedSessionOwner` / `SessionOwnerResolver`。
- context 查询与 launch 同源，不保留 route 层独立重建 VFS / capability / context 的主线。
- fallback 必须显式可审计，不能藏在 pipeline 中。
- runtime turn、eventing、hooks/effects、pending command 独立成边界。
- terminal event 先持久化，effect 进入 durable outbox。
- pending capability/runtime transition 不再藏在 `SessionMeta`。
- `working_dir` 必须类型化并收紧到 mount 内相对路径。

## 背离点

| 参考 review 目标 | 当前目标 |
|---|---|
| `LaunchPlan` 是不可变执行计划中心 | `SessionConstructionPlan` 是 session 构建事实源，`LaunchExecution` 是一次 launch 执行计划 |
| `LaunchPlanner` 解析 owner、context、VFS、capability、restore、hook | 目标态只要求 construction 与 launch 两个数据边界；planner/resolver 是实现细节 |
| `ContextComposer` / `SessionCompositionPlan` 作为 query/launch 同源 | `SessionConstructionPlan` 作为 query/launch/audit 同源，避免与 domain `SessionComposition` 混淆 |
| `ExecutionContext` 从 `LaunchPlan` 构建 | connector input 可作为 `LaunchExecution` 字段存在，`ExecutionContext` 只是 connector SPI 投影 |
| `SessionHub` 可退化为 facade 或删除 | 最终删除有职责的 `SessionHub`；迁移期 wrapper 只能转发，不能承载业务判断 |
| pending command 可为 command store / event stream | runtime command 事实源为 domain event，projection 只是可重建索引 |
| terminal effect router 订阅 terminal event | terminal event + durable outbox + 有限重试 + dead-letter |

## 采用参考 review 的内容

- 多入口统一不是只统一最终函数，而是统一前置决策数据流。
- API route 必须薄化。
- owner priority 分裂必须收掉。
- route-local finalizer 与 assembler finalizer 不能长期并存。
- `start_prompt_with_follow_up` 不能继续混合规划、状态写入、connector 调用、runtime supervision 和 terminal effects。
- `SessionPersistence` 需要拆出 meta / event / projection / outbox / runtime-command projection 的语义边界。

## 当前目标补充的内容

- 明确 `Turn` 不是主要业务边界，Turn 只负责运行态监督。
- 明确 session 构建需要自己的事实源：`SessionConstructionPlan`。
- 明确 `LaunchPlan` 若继续使用该名字，只能等价于 `LaunchExecution`，不能成为跨域事实源。
- 明确 context endpoint、audit、inspector 都投影 construction，而不是投影 launch。
- 明确 effect outbox 的 retry/dead-letter 与 idempotency 要求。
- 明确不保留 `LaunchResolution` / `ExecutionPlan` / `ExecutionProjector` 作为目标态必需中间层。
