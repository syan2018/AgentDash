# Lifecycle 控制面最终收束

## Goal

作为巨型 Lifecycle 控制面重构 PR 合并前的最终 review 父任务，统一确认剩余 active tasks 是否完成并收口归档。本任务不再承载已经归档的具体实现切片；它只负责最终验收、残留扫描、spec 一致性和归档判定。

## Current Baseline

已完成并归档：

- RuntimeSession 到 Frame / Assignment 的直接锚定。
- FrameLaunchEnvelope 与 Session launch boundary。
- 前端 Session runtime frame query。
- Graphless 默认 Agent Runtime。
- Session-Agent channel integration。
- Session Runtime control source convergence。
- Lifecycle control-plane closeout review。

仍需完成：

- `06-02-scoped-lifecycle-artifacts`
- `06-02-lifecycle-run-active-projection-structure`
- `06-03-database-business-semantic-convergence`
- `06-01-lifecycle-control-plane-concept-alignment` 的最终概念/实现一致性 review。

## Requirements

- 等剩余 implementation tasks 完成后，再执行本任务的最终 review。
- 对照 `06-01-lifecycle-control-plane-concept-alignment` 的目标模型，确认当前代码、contracts、migration、frontend runtime view 和 specs 一致。
- 对明确由其它任务承接的历史残留，不在本任务重新实现；只记录承接关系和当前 merge 风险。
- 对 PR 合并有风险的残留，必须输出具体阻断项、文件路径和建议 task owner。

## Acceptance Criteria

- [ ] `06-02-scoped-lifecycle-artifacts` 已完成或明确不阻断合并。
- [ ] `06-02-lifecycle-run-active-projection-structure` 已完成或明确不阻断合并。
- [ ] `06-03-database-business-semantic-convergence` 已完成必要 slice，或明确剩余字段不阻断本 PR。
- [ ] `06-01-lifecycle-control-plane-concept-alignment` 已完成最终 review 记录。
- [ ] contracts / generated TS 与当前 backend DTO 一致。
- [ ] migration baseline 与项目当前目标事实源一致。
- [ ] specs 只记录目标不变量和原因，不记录一次性任务日志。
- [ ] final residual scans 已执行并记录结果。

## Final Residual Scan

```bash
rg "list_by_session|SessionBinding|lifecycle_step_key" crates packages
rg "HookSessionRuntime|SessionHookSnapshot|companion_context|CompanionWaitRegistry" crates .trellis/spec
rg "active_node_keys|current_activity_key" crates packages .trellis/spec
rg "list_port_outputs|write_port_output|load_port_output_map|activity_outputs_from_port_map" crates
rg "WorkflowContract|step_key" crates packages .trellis/spec
rg "executor_config_json|tab_layout_json|task_count|is_default_for_task" crates packages .trellis/spec
```

## Out Of Scope

- 不重新实现已归档任务。
- 不引入旧 API / 旧 schema 兼容路径。
- 不把数据库业务语义宽任务全部强行塞进本父任务；只判断 PR 合并前阻断面。
