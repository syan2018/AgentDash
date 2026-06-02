# Lifecycle 控制面长链路收敛与 Frame 化

## Goal

围绕 Lifecycle 控制面收敛 Agent / Frame / RuntimeSession / Activity / Artifact 的事实归属，减少迁移期留下的 session-first、run-first、cache-first 长链路。目标状态是：Frame 层锚定 runtime session 的可执行事实，Assignment 层锚定 Activity attempt 的执行事实，Session 层只承担 turn 运行、connector accepted、stream ingestion、terminal cleanup 等运行监督职责。

## User Value

- Agent 执行链路更容易推理：从业务 subject 到 run、agent、frame、assignment、runtime trace 的路径清晰且唯一。
- Workflow 多 graph instance、Agent reuse、Frame hot update、capability grant 等场景不再依赖隐含 fallback 或启发式反查。
- 前后端运行态查询围绕 Lifecycle / Frame read model 收敛，减少 session cache 与 frame cache 之间的错位。
- 后续重构可以按控制面事实源逐段推进，而不是继续在 Session 逻辑里堆叠 owner/context/capability/session 解析。

## Confirmed Facts

- `RuntimeSession` terminal 回调当前通过 `RuntimeSession -> AgentFrame -> LifecycleAgent -> list AgentAssignment -> select_assignment_for_runtime_frame -> LifecycleRun` 反查 Activity attempt；`AgentAssignment` 已经具备 `run_id + graph_instance_id + activity_key + attempt + agent_id + frame_id` 的精确锚点。
- lifecycle VFS mount 已经携带 `run_id + graph_instance_id`，但 output port 写入和读取仍使用 `LifecycleRun + port_outputs/{port_key}`，缺少 `graph_instance_id / activity_key / attempt` 维度。
- 前端 `useSessionRuntimeState` 当前通过遍历本地 `frames.runtime_session_refs` 和 `agents.current_frame_id` 推导 session 对应 frame；后端已经具备 session trace 到 frame 的查询入口。
- `RuntimeLaunchRequest` 同时承载 frame surface、command prompt、identity、context bundle、capability/VFS/MCP typed projection、working directory、terminal hook binding 等不同层级事实，并在 construction / planner 中继续补齐 optional 字段。
- `LifecycleRun.lifecycle_id` 与 `active_node_keys` 仍保留单 root graph / 当前节点投影语义；真实 Activity 状态已经在 `WorkflowGraphInstance.activity_state` 中。
- Session launch 中仍有大量 owner/context/capability/runtime fact 的长流程管理；在 Frame 基本锚定 Session 事实后，这些解析应上提到 Frame construction / Frame launch envelope 层。

## Requirements

- 明确 Frame 层的目标职责：Frame construction 负责 owner、context、capability、VFS、MCP、execution profile、runtime delivery refs 的解析与投影；Session launch 只消费已解析的 frame launch envelope。
- 明确 Session 层保留职责：turn claim / activate / cancel / cleanup、connector accepted boundary、stream ingestion、terminal event persist、effect outbox dispatch、backend execution lease 生命周期。
- 建立 RuntimeSession 到 Frame / Assignment 的直接锚定路径，terminal callback 不再通过 run 级 assignment 列表和启发式选择推导 Activity attempt。
- 将 lifecycle artifact / output port 存储改为 graph instance / activity / attempt scoped，使 completion policy、hook gate、artifact binding 读取同一份结构化输出事实。
- 前端 runtime state 查询以后端 frame/runtime read model 为准，不在本地 cache 中猜测 session 对应 frame。
- 拆分或重命名 `RuntimeLaunchRequest`，区分 Frame surface、Launch intent、Resolved frame launch envelope、Connector launch input。
- 重新定义 run-level active projection 的用途；需要保留时应使用结构化 `ActiveActivityRef`，避免字符串拼接成为业务判断事实源。
- 数据库迁移应服务目标状态；本项目处于预研期，不需要保留旧 API / 旧字段兼容路径。

## Child Task Map

| Child Task | Scope | Primary Output |
| --- | --- | --- |
| `06-02-runtime-session-frame-assignment-anchor` | RuntimeSession 到 Frame / Assignment 的直接锚定 | terminal / advance 能直接定位 Activity attempt |
| `06-02-scoped-lifecycle-artifacts` | lifecycle VFS output 与 artifact binding 作用域化 | output port 按 graph/activity/attempt 存储 |
| `06-02-frame-launch-envelope-session-boundary` | Session 启动解析上提到 Frame construction | launch-ready `FrameLaunchEnvelope`，Session 只管 turn runtime |
| `06-02-frontend-session-runtime-frame-query` | 前端 session runtime 查询改为后端 frame read model | `/sessions/{runtime_session_id}/frame-runtime` 与前端 hook 收敛 |
| `06-02-lifecycle-run-active-projection-structure` | run-level active projection 结构化 | `ActiveActivityRef` 或 read-builder 派生投影 |
| Parent final integration | 跨 child 不变量与 spec 收口 | 确认所有 child 合并后事实源一致 |

## Start Order

| Order | Task | Why First / After | Completion Gate |
| --- | --- | --- | --- |
| 1 | `06-02-runtime-session-frame-assignment-anchor` | Frame / Assignment anchor 是 terminal callback、frontend session frame query、ContinueRoot reuse 的共同事实基础。先消除启发式 assignment 反查，后续任务才能复用同一锚点。 | `runtime_session_id` 能直接解析到 frame anchor；activity runtime session 能直接解析到 assignment / attempt anchor；reused runtime session 有明确 active anchor 规则。 |
| 2 | `06-02-scoped-lifecycle-artifacts` | Artifact scope 与 anchor 同属 Activity attempt identity；在 anchor 明确后，VFS write、completion gate、hook gate 可以按同一 attempt ref 读写。 | 同一 run 内多个 graph instance / 多个 attempt 的同名 port 不互相覆盖或误放行。 |
| 3 | `06-02-frontend-session-runtime-frame-query` | 前端 endpoint 应复用第 1 步的后端 anchor；第 2 步不是硬依赖，但完成后 runtime view 能展示更可靠的 attempt / artifact 状态。 | `useSessionRuntimeState` 只通过后端 `GET /sessions/{runtime_session_id}/frame-runtime` 获取 frame runtime，不再本地猜测 frame。 |
| 4 | `06-02-frame-launch-envelope-session-boundary` | 这是最大收口项，触及 Session launch 中心路径。等 anchor 与 artifact scope 稳定后再把 owner/context/capability/VFS/MCP 解析上提到 Frame construction，风险更可控。 | Session planner 消费 launch-ready `FrameLaunchEnvelope`；基础控制面事实不再由 Session launch 补齐。 |
| 5 | `06-02-lifecycle-run-active-projection-structure` | run-level active projection 是尾部清理项，最好等前面任务已经把业务路径迁到 assignment / graph instance / frame 后再处理。 | 后端业务路径不依赖字符串 `active_node_keys`；read model 使用结构化 `ActiveActivityRef` 或 graph instance attempt 派生。 |
| 6 | Parent final integration | 所有 child 完成后统一检查 specs、contracts、migrations、frontend/backend read model 是否仍有 session-first / run-first 残留。 | 父任务 acceptance criteria 全部可验证通过。 |

## Acceptance Criteria

- [ ] 形成设计文档，明确 `LifecycleRun`、`WorkflowGraphInstance`、`LifecycleAgent`、`AgentFrame`、`AgentAssignment`、`RuntimeSession`、artifact store 的权威职责。
- [ ] 形成执行计划，按可验证切片处理 session-to-frame anchoring、activity assignment anchoring、scoped port artifacts、frame launch envelope、frontend runtime query、run active projection。
- [ ] 实现后，RuntimeSession terminal / `complete_lifecycle_node` 能直接定位 Activity attempt，不依赖 run 级 assignment 列表扫描或 fallback。
- [ ] 实现后，output port 写入、completion gate、hook gate、artifact binding 使用 graph/activity/attempt scoped artifact 数据。
- [ ] 实现后，Session launch 不再承担 owner/context/capability/VFS/MCP 的长流程解析；这些事实由 Frame construction / resolved envelope 提供。
- [ ] 实现后，前端 session runtime state 不通过本地 frame cache fallback 猜测 frame。
- [ ] 实现后，相关 contract、migration、backend unit/integration test、frontend store/hook test 覆盖新的锚定关系。

## Out Of Scope

- 不在本任务中重新设计完整 WorkflowGraph 编辑体验。
- 不把 RuntimeSession 恢复为业务所有权事实源。
- 不保留旧 session-first endpoint 或旧 artifact path 的兼容访问层；若存在存量数据，仅通过迁移进入目标结构。

## Open Questions

- 当前无阻塞问题；启动顺序已在 `Start Order` 固化。若实施中发现 `FrameLaunchEnvelope` 必须先定义类型才能支撑 anchor endpoint，可回到 planning 更新第 1 / 第 4 阶段边界。
