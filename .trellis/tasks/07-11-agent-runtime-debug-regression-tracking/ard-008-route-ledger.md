# ARD-008 AgentRun Cutover Route Ledger

证据基线：commit `af21f9d7c` 删除了原`workspace/query.rs`、command policy、
RuntimeSession delivery/control模块与对应route handler，但前端consumer仍然存在。
每条route只有同时具备current owner、generated contract与定向验证才算完成切换。

| 前端consumer | HTTP route | 当前application owner / facts | Generated contract | 状态 |
| --- | --- | --- | --- | --- |
| `fetchAgentRunWorkspace` / `useAgentRunWorkspaceState` | `GET /agent-runs/{run_id}/agents/{agent_id}/workspace` | `AgentRunProductQuery` application use case组合`LifecycleReadModelQueryPort`、current `AgentFrame`/model config与`VfsSurfaceResolver(AgentRun)`；Runtime snapshot被明确排除并继续归Managed Runtime inspect所有。 | `AgentRunProductView` | ARD-008 foundation已实现并完成产品验证 |
| `fetchAgentRunRuntimeInspect` | `GET /agent-runs/{run_id}/agents/{agent_id}/runtime` | `AgentRunRuntime` facade -> Managed Runtime snapshot/binding | Agent Runtime generated contract与typed API response | 已存在；前端加载错误已与产品投影隔离 |
| `streamAgentRunRuntimeEvents` | `GET /agent-runs/{run_id}/agents/{agent_id}/runtime/events/stream/ndjson` | `AgentRunRuntime::read_events`，仅durable的有限replay | Agent Runtime generated event contract | ARD-008前序切片已修复并验证 |
| `submitAgentRunComposerInput` | `POST /agent-runs/{run_id}/agents/{agent_id}/composer-submit` | Runtime mailbox/facade admission与current Runtime snapshot guard；idle进入TurnStart，active command只携带typed steer intent | `AgentRunComposerSubmitRequest`，既有Run不允许executor/backend override | 已完成；真实首轮、第二轮与工具后follow-up连续验证 |
| `cancelAgentRun` / context compact | `POST .../cancel`, `POST .../runtime/context/compact` | `AgentRunRuntime` facade从Managed Runtime inspect生成current guard | `AgentRunRuntimeCommandRequest` | current routes；替换退役workspace command-precondition DTO |
| Runtime context popup | `GET .../runtime/context` | `AgentRunRuntime::read_context`返回canonical`RuntimeContextView` | Agent Runtime context contract | 已完成；删除旧projection route/fallback，target generation隔离迟到响应 |
| Runtime interaction cards | `POST .../runtime/interactions/{interaction_id}/respond` | `AgentRunRuntime::resolve_interaction` + current snapshot availability | generated `InteractionResponse` union | 已完成；approval/user input/MCP/dynamic tool共用generic route |
| Project AgentRun list state | `GET /projects/{project_id}/agent-runs` | `ProjectAgentRunListQuery`组合LifecycleRun/LifecycleAgent、ProjectAgent identity、subject association、canonical `AgentLineage` forest与Managed Runtime inspect；单个无binding保持`runtime=None`，inspect错误带run/agent坐标失败。 | `ProjectAgentRunListView` | ARD-008 list切片已完成产品验证；侧栏/Agent Hub均恢复6条且导航命中既有成功Run |
| AgentRun删除动作 | 无route/consumer | 当前没有canonical delete command owner，因此产品面不展示该动作 | 无删除合同 | 已收束：删除假按钮、service与response contract |
| legacy mailbox list/content/reorder/promote/resume UI | 无route/consumer | canonical Runtime mailbox只保留composer/gate delivery与durable scheduler职责 | 仅保留当前submit response中的narrow mailbox identity | 已收束：管理UI、props、adapter与退役contracts删除 |
| Runtime feed | `GET .../runtime/events/stream/ndjson` | snapshot baseline + durable Runtime event replay | Agent Runtime event contract | 已收束：旧`journal/*` transport/reducer/validator与fallback删除 |
| fork | 无AgentRun产品route/consumer | 未来必须先建立canonical `ThreadFork`、product child binding与availability | 当前无合同 | 明确不在当前产品面制造入口，不保留dead consumer |
| detail projection parent/children display | `GET .../workspace` | `AgentRunProductQuery`直接读取canonical `AgentLineage`，投影parent与递归children；与list共享title projector和16层上限 | `AgentRunProductView.lineage` | 已完成；cycle/orphan/cross-run/depth测试通过 |

## Workspace projection数据流

```text
GET /workspace
  -> 鉴权LifecycleRun所属Project
  -> AgentRunProductQuery
       -> LifecycleReadModelQueryPort(run_id)
            -> AgentRunView + subject associations
       -> AgentFrameRepository::get_current(agent_id)
            -> frame ref + capability/context/VFS/MCP + execution profile
            -> ConversationModelConfigResolver -> typed model_config
       -> VfsSurfaceResolver(ResolvedVfsSurfaceSource::AgentRun)
            -> BusinessResourceSurfaceQuery -> current AgentFrame VFS
            -> resolved mount/backend/edit facts
  -> AgentRunProductView

GET /runtime（独立请求）
  -> AgentRunRuntime facade
  -> Managed Runtime binding + snapshot
```

`AgentRunWorkspaceView`混合了退役RuntimeSession source anchor、workspace command policy、
mailbox snapshot与conversation execution authority，因此本切片将其删除。替代合同只承载current
product facts；Runtime state保持为独立加载的canonical projection。

## Project AgentRun list数据流

```text
GET /projects/{project_id}/agent-runs?limit&cursor
  -> Project Use授权
  -> ProjectAgentRunListQuery
       -> LifecycleRunRepository.list_by_project + run activity keyset
       -> LifecycleAgentRepository.list_by_run
       -> AgentLineageRepository.list_by_run -> guarded recursive child forest
       -> ProjectAgentRepository.list_by_project -> product identity label
       -> LifecycleSubjectAssociationRepository -> subject identity/label
       -> AgentRunRuntime.inspect(run_id, agent_id)
            -> optional canonical thread_status + active_turn_id
  -> ProjectAgentRunListView
```

旧`AgentRunWorkspaceListView/Entry`依赖`AgentRunWorkspaceShell.delivery_status`，其producer读取
已退役RuntimeSession delivery/session meta，因此没有复用。新合同只保留当前consumer读取的title、
Lifecycle status、last activity、optional Runtime summary、subject与真实lineage children；不携带
未消费的frame/run status副本，也不输出仿真的delivery status。
