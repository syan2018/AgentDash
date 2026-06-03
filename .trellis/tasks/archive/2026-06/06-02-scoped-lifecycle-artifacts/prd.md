# Lifecycle Artifact 输出按 Activity Attempt 作用域化

## Goal

把当前仍以 run-level port output 为事实源的 lifecycle artifact 路径收束为 Activity attempt scoped output。当前代码已经有 `ActivityOutputArtifact { activity_key, attempt, port_key }` 这类结构化 read model，但写入、completion、hook gate 和 VFS 仍可经由 `run_id + port_key` 读取同一 run 下的扁平 output map。本任务只处理这个剩余差距。

目标状态：output port 写入、completion policy、hook `port_output_gate`、artifact binding、read model aggregation 都消费同一个 `run_id + graph_instance_id + activity_key + attempt + port_key` 输出事实。

## Current Baseline

- `ActivityOutputArtifact` 已在 contracts / generated TS 中暴露 `activity_key + attempt + port_key`。
- lifecycle mount metadata 已能携带 `run_id + graph_instance_id`。
- `JourneyService::write_port_output` / `list_port_outputs` 仍以 `run_id + port_key` 读写。
- `provider_lifecycle.rs` 仍通过 `list_port_outputs(run_id)` 支撑 VFS read。
- `orchestrator.rs` 仍通过 `activity_outputs_from_port_map` 从 run-level map 生成 activity outputs。
- 本项目仍处于预研期，schema / API / generated contracts 可以直接进入目标形态，不需要运行时兼容旧 path。

## Requirements

- 定义 typed scoped artifact ref，至少包含 `run_id`、`graph_instance_id`、`activity_key`、`attempt`、`port_key`。
- 替换 run-level `write_port_output` / `list_port_outputs` / `load_port_output_map` 生产路径。
- lifecycle VFS 写入 `artifacts/{port_key}` 时必须解析当前 graph instance、activity 和 attempt。
- completion policy 只检查当前 activity attempt 的 declared output ports。
- hook `port_output_gate` 与 completion policy 使用同源 scoped outputs。
- artifact binding 从上游 scoped output 生成下游 scoped input；latest/history alias policy 必须显式。
- read model 可以按 run 聚合 scoped artifacts，但聚合结果不得成为 runtime fact source。
- migration 直接修改当前 baseline / dev migration 到目标结构，不保留旧 path 运行时兼容分支。

## Acceptance Criteria

- [ ] 生产代码中 run-level `write_port_output` / `list_port_outputs` / `load_port_output_map` 不再作为 runtime output fact source。
- [ ] 同一 run 内两个 graph instance 写同名 `port_key` 时互不覆盖。
- [ ] 同一 activity 多 attempt 写同名 `port_key` 时保留 history，并能按 latest policy 解析。
- [ ] `complete_lifecycle_node` 缺少 required output 时只检查当前 attempt。
- [ ] hook `port_output_gate` 使用 scoped output 集合。
- [ ] lifecycle VFS 的同名 output port read/write 均以 graph/activity/attempt 定位。
- [ ] artifact binding 使用 scoped output ref 生成 scoped input ref。
- [ ] contracts / generated TS / tests 与目标 scoped artifact 模型一致。

## Out Of Scope

- 不重新设计 VFS provider 总体协议。
- 不新增面向用户的 artifact 管理 UI。
- 不保留旧 `port_outputs/{port_key}` 运行时读取兼容。
