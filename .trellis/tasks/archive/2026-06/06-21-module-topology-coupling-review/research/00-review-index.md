# 第一轮 review 索引

## 覆盖状态

第一轮已由 6 个 `trellis-research` subagents 并发完成。主会话只做索引和调度，不补写模块 review 结论。

| File | Slice | Coverage | Next Use |
| --- | --- | --- | --- |
| `research/01-backend-layer-topology.md` | 后端分层骨架 | crate 依赖、API/Application/Domain/Infrastructure/SPI/Contracts、composition root、RepositorySet、contract DTO 边界 | 第二轮 contract boundary 与 route/application boundary 深挖 |
| `research/02-workflow-lifecycle-task-topology.md` | Workflow / Lifecycle / Task | WorkflowGraph definition、LifecycleRun/orchestration、runtime reducer、AgentFrame/session anchor、SubjectExecutionView、Task/Story projection | 第二轮 lifecycle runtime fact source 与 subject execution 深挖 |
| `research/03-session-agentrun-runtime-topology.md` | Session / AgentRun / Runtime | Launch pipeline、RuntimeSession、Backbone stream、AgentRun workspace、Mailbox、AgentRuntimeDelegate、frontend chat boundary | 第二轮 AgentRun command/control 与 anchor consistency 深挖 |
| `research/04-capability-permission-extension-vfs-topology.md` | Capability / Permission / Extension / VFS | AgentFrame capability/VFS/MCP surface、PermissionGrant、RuntimeGateway、extension action/channel、Canvas workspace module | 第二轮 permission/frame/VFS/gateway admission 深挖 |
| `research/05-local-relay-desktop-topology.md` | Local / Relay / Desktop | desktop boot、backend ensure/register、relay protocol、workspace routing、backend execution placement、local command handlers、extension host | 第二轮 backend placement / lease / extension backend target 深挖 |
| `research/06-frontend-contracts-topology.md` | Frontend / Contracts | app route/page、services、generated DTO、stores/hooks、Session stream、AgentRun workspace、VFS/Extension bridge、desktop/extension packages | 第二轮 generated contract / frontend handwritten fact source 深挖 |

## 第一轮共同 caveat

- 多数 subagents 在启动时看到的 `prd.md` 仍是占位版本；它们按调度 prompt、`design.md`、`implement.md` 和指定 spec 执行。
- 多数 subagents 的 shell 中 `task.py current --source` 未解析出 active task；它们按显式 task path 写入了本目录。
- 所有产物均为静态架构研究，没有修改业务代码，没有运行测试。

## 第二轮并发 deep-dive 拆分

第二轮只围绕第一轮产出的高风险交叉耦合，不重复 06-14 overdesign review 的旧结论。

| Output | Reviewer Focus | Source Files |
| --- | --- | --- |
| `research/10-contract-boundary-deep-dive.md` | application/contracts/API/frontend generated DTO 与手写 DTO/stream 的边界 | `01`, `06` |
| `research/11-agentrun-control-deep-dive.md` | AgentRun command/control、ConversationSnapshot、Mailbox、RuntimeSession runtime-control、direct steer | `03`, `06`, `02` |
| `research/12-lifecycle-runtime-facts-deep-dive.md` | Lifecycle start/drain、status aggregation、SubjectExecutionView、Task execution surfaces、RuntimeSessionExecutionAnchor | `02`, `03` |
| `research/13-permission-frame-vfs-gateway-deep-dive.md` | PermissionGrant、AgentFrame capability/VFS surface、Canvas expose、RuntimeGateway action/channel admission | `04`, `03`, `06` |
| `research/14-local-placement-relay-deep-dive.md` | backend execution placement、lease cleanup、workspace routing、extension backend target、Relay/local/desktop boundary | `05`, `04`, `06` |

## 预期最终产物

- `research/coupling-matrix.md`：跨模块耦合矩阵。
- `research/followup-backlog.md`：后续 Trellis task 候选，按 P0/P1/P2 排序。
- 必要时由主会话继续创建子 task，但只在某个 backlog 项已经具备独立验收范围时创建。
