# Prompt Pipeline 阶段边界收敛

## Goal

把当前 `agentdash-application::session` 的 prompt launch 主链路从含糊的 `prompt_pipeline.rs` 收敛为清晰、可测试、可继续演进的 `session-launch` 子域。

这个任务不只是文件拆分，而是要让代码语言本身表达：

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchPlan
  -> PreparedTurn
  -> ConnectorAcceptedTurn
  -> CommittedTurn
  -> AttachedTurn
```

最终收益是让 lifecycle activity、resume、branch、multi-agent、tool approval replay、runtime command replay 等后续能力能落到明确阶段，而不是继续堆进一个 pipeline 函数。

## User Intent

- 用户希望做更深入、完整的调整，而不是只做行为保持的文件级抽取。
- 用户允许必要的重命名；重命名必须让系统更清晰，不能只是审美改名。
- 项目仍处于预研阶段，不需要保留兼容性别名或旧路径 fallback；如果内部 Rust API 名称需要调整，应一次性改到正确状态。

## Confirmed Facts

- `runtime-control-plane-review.md` 建议拆 `prompt_pipeline.rs` 的 launch 阶段，明确 `plan / prepare / start / commit / attach`。
- `platform-boundary-governance-review.md` 建议把 session 拆为 `core / launch / runtime / capability / hooks / effects` 子域，短期先在 `agentdash-application/src/session/` 内建立目录和 facade。
- `.trellis/spec/backend/session/session-startup-pipeline.md` 已规定主线为 `LaunchCommand -> SessionConstructionPlan -> LaunchExecution -> ExecutionContext -> SessionEvent / TerminalEffectOutbox`。
- 当前代码已有 `LaunchCommand`、`SessionConstructionPlan`、`SessionLaunchPlanner`、`LaunchExecution`、`SessionLaunchService`。
- 当前 `prompt_pipeline.rs` 约 1292 行，仍包含：
  - construction provider 调用与 prompt claim；
  - launch planning 调用；
  - runtime/direct MCP/relay MCP tools 准备；
  - context frame 准备与 transform_context enqueue；
  - turn activation；
  - pending runtime transition application；
  - `connector.prompt`；
  - user/start/context/capability/meta/runtime-command commit；
  - title generation；
  - `SessionTurnProcessor` 与 stream adapter attach；
  - hook runtime reload/helper 逻辑。
- Relay prompt 前注册 session sink 的 review 建议已落地并有测试覆盖，不属于本任务主范围。

## Before State

重构前的状态不是“缺少几个 helper”，而是 session launch 的关键语言和代码边界都还没有对齐：

- `prompt_pipeline.rs` 是事实上的巨型 launch orchestrator，但文件名仍停留在泛化 pipeline 语义。
- `execute_constructed_launch` 名称只暗示“执行已构建 launch”，实际却覆盖 preparation、connector start、accepted commit、stream ingestion 多个阶段。
- `LaunchExecution` 名称容易误导为“已经执行中的对象”，但它当前更像 launch 决策结果和 connector context projection。
- connector accepted 前后的副作用顺序主要依赖大函数内的人工顺序，而不是由类型流约束。
- user/start/context/capability/meta/runtime-command/title side effects 分散在同一段执行逻辑里，无法单独审查 commit 边界。
- stream attach 与 accepted commit 混在同一个 orchestration 段里，未来调整 stream/relay/terminal 时容易牵动 launch planning。
- hook runtime reload/helper 仍通过 `prompt_pipeline.rs` 扩展 `SessionRuntimeInner`，说明 launch、hooks、runtime wiring 之间边界不够清晰。
- `SessionLaunchDeps` 是宽依赖包，每个阶段可见过多无关能力，测试构造和职责审计都偏重。

这个 before state 是本任务最后 review 的对照基线：最终代码不能只是把同一坨逻辑搬进新文件，而要消除这些边界问题。

## Expected After State

重构后的预期状态必须清晰到跨上下文压缩后也能重新对齐：

- `session/launch/` 是唯一 session launch 实现区域，模块名能表达职责：
  - `command.rs`：来源意图。
  - `plan.rs` / `planner.rs`：launch 决策结果。
  - `orchestrator.rs`：阶段编排。
  - `preparation.rs`：accepted 前 turn/context/tools 准备。
  - `connector_start.rs`：`connector.prompt` accepted 边界。
  - `commit.rs`：accepted 后事实提交。
  - `ingestion.rs`：stream 到 processor 的桥接。
  - `service.rs`：外部 application service facade。
- 核心类型流表达阶段顺序：

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchPlan
  -> PreparedTurn
  -> ConnectorAcceptedTurn
  -> CommittedTurn
  -> AttachedTurn
```

- commit 阶段必须消费 `ConnectorAcceptedTurn`，从类型上表达“未 accepted 不能提交成功事实”。
- `prompt_pipeline.rs`、`execute_constructed_launch`、误导性的 `LaunchExecution` 命名不再出现在核心 launch 路径。
- `SessionConstructionPlan` 继续是事实源；`ExecutionContext` 继续只是 connector projection。
- `SessionRuntimeInner` 不再被 launch 文件补 hook/runtime helper；hook reload/refresh 逻辑下沉到 hooks/runtime 边界。
- `SessionLaunchDeps` 至少被隔离在 launch 子域内；如果阶段边界稳定，应进一步拆为阶段窄依赖。
- 最终 review 能通过搜索和测试证明：旧 pipeline 边界退出、阶段类型存在、accepted 前后副作用语义没有漂移。

## Requirements

- 将 `prompt_pipeline.rs` 代表的职责收敛到 `session/launch/` 子域；最终 `prompt_pipeline.rs` 应消失，或只短期作为薄 facade，不能继续承载实际阶段逻辑。
- 建立明确阶段类型，至少覆盖：
  - `LaunchPlan`：单轮 launch 决策结果。
  - `PreparedTurn`：connector accepted 前的 turn runtime/context/tools 准备结果。
  - `ConnectorAcceptedTurn`：`connector.prompt` 成功返回 stream 后的 accepted 边界。
  - `CommittedTurn`：accepted 后事件/meta/runtime-command commit 完成后的结果。
  - `AttachedTurn`：stream 已接入 `SessionTurnProcessor` 后的结果。
- 用类型流约束副作用顺序：commit 阶段必须消费 `ConnectorAcceptedTurn`，避免 connector 未 accepted 时提交成功事实。
- 允许重命名内部 Rust API 与模块路径；不保留旧名字兼容层，除非同一实现阶段为了迁移测试临时需要。
- 明确 public surface 分级：
  - Wire/API surface：HTTP、Relay JSON、BackboneEnvelope、NDJSON stream payload 默认不改变。
  - Application Rust surface：`session` 内部或 crate 内公开类型可以为清晰边界重命名，但必须全量更新调用点和测试。
  - Private launch internals：应积极重命名到最准确的阶段语言。
- `SessionConstructionPlan` 继续是构建事实源；launch 子域不能重新解析 owner、context、VFS、MCP 或 capability facts。
- `ExecutionContext` 继续是 connector-facing projection，不作为 application 事实源写回。
- 保持 connector accepted 前后的语义边界：
  - accepted 前可 claim、plan、prepare、activate runtime projection、准备 hook/context；
  - accepted 前失败不得提交 user message、`TurnStarted`、context/capability success event、bootstrap success、runtime command applied；
  - accepted 后再提交 user/start/context/capability/meta/runtime-command/title side effects。
- stream ingestion 只负责 `ExecutionStream -> SessionTurnProcessor` 桥接，不混入 launch planning 或 accepted commit。
- `SessionRuntimeInner` 继续瘦身；launch 所需依赖应逐步由 `SessionLaunchDeps` 收敛为阶段专用窄依赖。
- 每个阶段必须可以独立测试或通过现有高价值行为测试验证。

## Acceptance Criteria

- [ ] `crates/agentdash-application/src/session/launch/` 成为 session launch 子域入口，至少包含 command/plan/orchestrator/preparation/connector-start/commit/ingestion/service 这些职责边界。
- [ ] `prompt_pipeline.rs` 不再承载真实 orchestration；若保留，只能作为短期 re-export/facade，且 implement plan 需要明确何时删除。
- [ ] `execute_constructed_launch` 这类含糊的总控函数被更清晰的 orchestration flow 替代。
- [ ] 命名能区分 command / construction / plan / preparation / connector accepted / commit / ingestion，不再用泛化 `pipeline` 词汇描述具体阶段。
- [ ] `LaunchExecution` 的命名经过审计；若它仍只表达决策结果，应重命名为 `LaunchPlan` 或拆出更准确的结果类型。
- [ ] connector setup failure 仍会释放 turn runtime 并记录 failed terminal，不提交 user/start/context/capability/bootstrap success side effects。
- [ ] connector accepted 后仍按当前语义提交 user message、turn started、context frame、capability state changed、session meta、runtime command applied 与 title generation。
- [ ] pending runtime commands 只能在 connector accepted 后从 `requested` 进入 `applied`；applied commit 失败仍会改写为 failed 以避免下一轮重复应用。
- [ ] stream attach 后仍通过 `SessionTurnProcessor` 处理 notification 和 terminal，cancel/failed/completed 语义不变。
- [ ] hook runtime helper 从 `prompt_pipeline` 下沉到更合适的 hooks/runtime/launch stage 模块，`SessionRuntimeInner` 不继续被 prompt pipeline 扩展。
- [ ] 相关 session launch、connector setup failure、runtime command apply、relay early event 测试通过。
- [ ] 若实现中发现 durable 架构不变量变化，更新 `.trellis/spec/backend/session/`。
- [ ] 最终执行 `Final Review Check`：对照本 PRD 的 Before/After、两份原始 review 和最终代码形态，确认没有留下“换文件名但边界仍含糊”的半吊子重构。

## Out of Scope

- 不做 crate 级拆分，例如暂不新增 `agentdash-session` crate。
- 不重写 `SessionConstructionProvider` 或 owner/context construction 规则。
- 不处理 AppState/bootstrap 拆分。
- 不处理 Relay protocol 分域拆分。
- 不处理前端 stream 抽象。
- 不改变 HTTP/Relay/Backbone/NDJSON wire payload，除非后续单独任务明确要求。
- 不做与 session launch 边界无关的批量命名清理。

## Planning Decision

本任务按“单个复杂任务 + 明确阶段检查点”推进，而不是立即拆 parent/child task。原因是主要改动集中在同一条 session launch 主链，阶段间高度依赖同一组类型流。用户已确认当前看起来不需要额外拆任务。

若执行中发现某个阶段已经形成独立交付面，且继续放在本任务会让 review 或验证不可控，再从本任务拆出子任务。
