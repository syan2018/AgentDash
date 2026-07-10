# WI-08 Workflow / Channel Integration

Status: planned

Depends On: WI-03、WI-04、WI-06、Channel ref/admission stable

## Scope

- Workflow OperationScript call path 复用同一 executor。
- durable multi-step Interaction effect 通过 replay-safe `workflow.start` Operation 进入 Workflow，不把 OperationScript 变成 durable job。
- Interaction attention 通过 Channel typed ref/summary 投递。
- OperationScript、Interaction/EffectIntent、Component、Canvas distribution/resource 四组端到端验收场景。

## Exit Criteria

- Workflow 不复制 script executor 或 step runtime。
- Channel 不拥有 Interaction command/event/state。
- Mailbox/external delivery 只承载 attention/handoff。
- 四组验收场景覆盖 User/Agent、即时/可靠 effect 与 Personal/Project 路径。

## Validation

- workflow compiler/runtime tests。
- Channel attention/mailbox integration tests。
- browser/integration smoke。
