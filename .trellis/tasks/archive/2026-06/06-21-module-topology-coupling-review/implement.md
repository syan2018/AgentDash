# 项目主模块拓扑与耦合关系 review 执行计划

## Current Status

- 父任务已创建：`.trellis/tasks/06-21-module-topology-coupling-review`
- 第一轮 6 个 `trellis-research` subagents 已完成，输出 `research/01` 到 `research/06`。
- 第二轮 5 个 deep-dive `trellis-research` subagents 已完成，输出 `research/10` 到 `research/14`。
- Round 3 综合产物已完成：`research/coupling-matrix.md` 与 `research/followup-backlog.md`。
- 后续多模块收敛路线图维护在 `roadmap.md`，用于追踪 Contract Boundary、Runtime Coordinate、Capability Exposure、Control Surface 与 Runtime Failure 的跨簇进度和阻塞关系。
- 当前 task 是只读 review 编排，不进入代码实现。

## Round 1: 主模块拓扑盘查

并发启动以下 subagents：

| Agent | Scope | Output |
| --- | --- | --- |
| backend-layer-topology | 后端 crate/分层骨架 | `research/01-backend-layer-topology.md` |
| workflow-lifecycle-task | Workflow / Lifecycle / Task / Story execution | `research/02-workflow-lifecycle-task-topology.md` |
| session-agentrun-runtime | Session / RuntimeSession / AgentRun / mailbox / hooks | `research/03-session-agentrun-runtime-topology.md` |
| capability-permission-extension-vfs | Capability / Permission / Extension / VFS / RuntimeGateway | `research/04-capability-permission-extension-vfs-topology.md` |
| local-relay-desktop | Local backend / Relay / Desktop shell / workspace routing | `research/05-local-relay-desktop-topology.md` |
| frontend-contracts-topology | Frontend feature topology / generated contracts / streams | `research/06-frontend-contracts-topology.md` |

Round 1 完成条件：

- [x] 6 个输出文件全部存在。
- [x] 每个输出文件都包含模块清单、主链路拓扑、耦合点、下一轮深挖问题和既有 review 覆盖说明。
- [x] 主会话生成 `research/00-review-index.md`，只索引产物和覆盖缺口，不自行补 review 结论。

## Round 2: 高风险耦合交叉深挖

Round 2 的 subagents 由 Round 1 输出决定，已拆为：

| Agent | Scope | Output |
| --- | --- | --- |
| contract-boundary | application/contracts/API/frontend generated DTO 与手写 DTO/stream 边界 | `research/10-contract-boundary-deep-dive.md` |
| agentrun-control | AgentRun command/control、ConversationSnapshot、Mailbox、RuntimeSession runtime-control、direct steer | `research/11-agentrun-control-deep-dive.md` |
| lifecycle-runtime-facts | Lifecycle start/drain、status aggregation、SubjectExecutionView、Task execution surfaces、RuntimeSessionExecutionAnchor | `research/12-lifecycle-runtime-facts-deep-dive.md` |
| permission-frame-vfs-gateway | PermissionGrant、AgentFrame capability/VFS surface、Canvas expose、RuntimeGateway action/channel admission | `research/13-permission-frame-vfs-gateway-deep-dive.md` |
| local-placement-relay | backend execution placement、lease cleanup、workspace routing、extension backend target、Relay/local/desktop boundary | `research/14-local-placement-relay-deep-dive.md` |

Round 2 输出命名：

- [x] `research/10-contract-boundary-deep-dive.md`
- [x] `research/11-agentrun-control-deep-dive.md`
- [x] `research/12-lifecycle-runtime-facts-deep-dive.md`
- [x] `research/13-permission-frame-vfs-gateway-deep-dive.md`
- [x] `research/14-local-placement-relay-deep-dive.md`

## Round 3: 汇总与后续任务候选

主会话基于 subagent 文件生成：

- [x] `research/coupling-matrix.md`
- [x] `research/followup-backlog.md`

`followup-backlog.md` 每个条目必须包含：

- 问题标题。
- 风险等级。
- 涉及模块。
- 当前耦合关系。
- 为什么影响后续开发。
- 建议拆出的 Trellis task scope。
- 验收方向。
- 来源 research 文件。

## Validation

本 task 不做代码验证。仅做以下轻量校验：

- `git status --short` 确认只有 task artifact 改动。
- 检查 `research/*.md` 文件数量与 Round 输出一致。
- 检查每个 subagent 输出是否引用具体文件路径或 spec 路径作为证据。

## Stop Conditions

- 如果某个 subagent 输出偏离范围，主会话只重新派发该 slice，不自行补写结论。
- 如果 Round 1 已发现明显 P0 架构风险，仍先完成 Round 1 覆盖，再进入 Round 2 深挖。
- 如果用户要求停止，本 task 保留当前 research 和计划，不启动新一轮 subagents。
