# ARD-008 AgentRun Cutover Route Ledger

证据基线：commit `af21f9d7c` 删除了原`workspace/query.rs`、command policy、
RuntimeSession delivery/control模块与对应route handler，但前端consumer仍然存在。
每条route只有同时具备current owner、generated contract与定向验证才算完成切换。

| 前端consumer | HTTP route | 当前application owner / facts | Generated contract | 状态 |
| --- | --- | --- | --- | --- |
| `fetchAgentRunWorkspace` / `useAgentRunWorkspaceState` | `GET /agent-runs/{run_id}/agents/{agent_id}/workspace` | `AgentRunProductQuery` application use case组合`LifecycleReadModelQueryPort`、current `AgentFrame`/model config与`VfsSurfaceResolver(AgentRun)`；Runtime snapshot被明确排除并继续归Managed Runtime inspect所有。 | `AgentRunProductView` | ARD-008 foundation已实现并完成产品验证 |
| `fetchAgentRunRuntimeInspect` | `GET /agent-runs/{run_id}/agents/{agent_id}/runtime` | `AgentRunRuntime` facade -> Managed Runtime snapshot/binding | Agent Runtime generated contract与typed API response | 已存在；前端加载错误已与产品投影隔离 |
| `streamAgentRunRuntimeEvents` | `GET /agent-runs/{run_id}/agents/{agent_id}/runtime/events/stream/ndjson` | `AgentRunRuntime::read_events`，仅durable的有限replay | Agent Runtime generated event contract | ARD-008前序切片已修复并验证 |
| `submitAgentRunComposerInput` | `POST /agent-runs/{run_id}/agents/{agent_id}/composer-submit` | Runtime mailbox/facade admission与current Runtime snapshot guard | `AgentRunComposerSubmitRequest`，已删除dead workspace stale-guard字段 | current route；foundation已替换request contract |
| `cancelAgentRun` / context compact | `POST .../cancel`, `POST .../runtime/context/compact` | `AgentRunRuntime` facade从Managed Runtime inspect生成current guard | `AgentRunRuntimeCommandRequest` | current routes；替换退役workspace command-precondition DTO |
| Project AgentRun list state | `GET /projects/{project_id}/agent-runs` | `ProjectAgentRunListQuery`组合LifecycleRun/LifecycleAgent、ProjectAgent identity、subject association、canonical `AgentLineage` forest与Managed Runtime inspect；单个无binding保持`runtime=None`，inspect错误带run/agent坐标失败。 | `ProjectAgentRunListView` | ARD-008 list切片已完成产品验证；侧栏/Agent Hub均恢复6条且导航命中既有成功Run |
| `deleteAgentRun` / `ActiveAgentRunList`删除动作 | `DELETE /projects/{project_id}/agent-runs/{run_id}` | 当前无command owner；不能恢复旧RuntimeSession delete cascade | 旧删除response仍存在 | open，独立切片决定迁移或删除consumer |
| legacy mailbox list/content/reorder/promote/resume UI | 已删除routes | 需要建立Managed Runtime/product mailbox owner，或连同consumer删除 | 退役mailbox contracts仍存在 | open；foundation未恢复 |
| journal history stream consumers | `/agent-runs/{run_id}/agents/{agent_id}/journal/*` | 需要迁移到canonical Runtime events/snapshot，或建立独立product journal owner | 旧Session contracts | open |
| fork | 已删除AgentRun fork route | 先建立canonical `ThreadFork`、product child binding与Runtime availability | 当前无合同 | open |
| detail projection parent/children display | current detail projection无对应字段；list children已由canonical `AgentLineage` forest恢复 | Lifecycle lineage repository是当前产品事实源，detail尚未纳入 | 当前无detail合同 | open；detail迁移前不展示对应UI分支 |

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
