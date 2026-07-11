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
| Project AgentRun list state | `GET /projects/{project_id}/agent-runs` | 需要从Lifecycle runs/agents与current Runtime summary重建；当前没有owner route | 待定义 | open，下一切片 |
| legacy mailbox list/content/reorder/promote/resume UI | 已删除routes | 需要建立Managed Runtime/product mailbox owner，或连同consumer删除 | 退役mailbox contracts仍存在 | open；foundation未恢复 |
| journal history stream consumers | `/agent-runs/{run_id}/agents/{agent_id}/journal/*` | 需要迁移到canonical Runtime events/snapshot，或建立独立product journal owner | 旧Session contracts | open |
| fork | 已删除AgentRun fork route | 先建立canonical `ThreadFork`、product child binding与Runtime availability | 当前无合同 | open |
| lineage parent/children display | current detail projection无对应字段 | Lifecycle lineage repository是当前产品事实源，本foundation未纳入 | 当前无合同 | open；迁移前不展示UI分支 |

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
