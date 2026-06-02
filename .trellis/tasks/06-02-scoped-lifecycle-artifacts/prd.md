# Lifecycle Artifact 输出按 Activity Attempt 作用域化

## Goal

将 lifecycle output port 与 artifact 存储从 run-level `port_outputs/{port_key}` 收敛为 Activity attempt scoped artifact。目标状态是 output port 写入、completion policy、hook gate、artifact binding、read model 都消费同一份 `graph_instance_id + activity_key + attempt + port_key` 结构化事实。

## User Value

- 多 graph instance 和同名 port 不互相污染。
- Activity retry / iteration 能区分 per-attempt 输出与 latest alias。
- 后续 artifact edge、gate evaluation、workflow projection 能基于结构化数据推进。

## Confirmed Facts

- lifecycle mount root 和 metadata 已包含 `run_id + graph_instance_id`。
- lifecycle VFS 写 output port 时只调用 `write_port_output(run_id, port_key, content)`。
- `load_port_output_map(repo, run_id)` 只读取 `LifecycleRun + port_outputs` 容器。
- `ActivityLifecycleRunState.outputs` 已经有 `activity_key + attempt + port_key + value`，但 artifact 输入路径还未 scoped。
- BeforeStop `port_output_gate` 自身只比较 required / fulfilled keys；污染来自 hook provider 给它填充的 `fulfilled_port_keys` 使用 run-level output map。

## Requirements

- scoped artifact key 必须包含 `graph_instance_id + activity_key + attempt + port_key`。
- 优先评估 first-class artifact repository；若继续复用 inline files，也必须通过 typed scoped helper 隐藏 container/path 拼接。
- lifecycle VFS 写入 `artifacts/{port_key}` 时必须从 active graph instance state 解析当前 activity/attempt。
- completion policy 只读取当前 activity attempt 的 declared output ports。
- hook gate 读取与 completion policy 同源的 scoped outputs。
- artifact binding 支持从上游 scoped output 生成下游 scoped input。
- read model 可以聚合 run 级 artifacts，但聚合不得作为 runtime fact source。
- 数据库 migration 直接进入目标结构，不保留旧 path 运行时兼容。

## Acceptance Criteria

- [ ] `write_port_output` / `load_port_output_map` 替换为 scoped artifact API。
- [ ] 多 graph instance 中相同 `port_key` 写入互不覆盖。
- [ ] 同一 activity 多 attempt 能区分输出，并按 alias policy 选择 latest / history。
- [ ] `complete_lifecycle_node` 缺少 required output 时只检查当前 attempt。
- [ ] hook `port_output_gate` 使用 scoped outputs。
- [ ] VFS 两个 graph mount 写同名 port 时独立读取。
- [ ] artifact binding 从 scoped output 生成 scoped input。
- [ ] 迁移和测试覆盖旧 run-level 容器清理。

## Out Of Scope

- 不重新设计 VFS provider 总体协议。
- 不引入面向用户的 artifact 管理 UI。
- 不保留旧 `port_outputs/{port_key}` 运行时读取兼容。

## Dependency Notes

- 可以独立于 FrameLaunchEnvelope 实施。
- 若 terminal anchor 已完成，可复用 activity anchor 解析当前 attempt；否则本任务需在 VFS active context 内自行定位 running attempt。
