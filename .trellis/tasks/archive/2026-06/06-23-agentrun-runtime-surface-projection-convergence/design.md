# AgentRun runtime surface 投影收束设计

## 目标不变量

runtime surface 的写入与 live adoption 必须只有一个控制面入口：

```text
(业务来源变化)
  -> RuntimeSurfaceUpdateRequest
  -> AgentRunRuntimeSurfaceUpdateService
  -> AgentRunSurfaceProjectionContext
  -> ProjectionResult
  -> AgentFrame revision
  -> active runtime adoption
  -> semantic ContextFrame / frontend invalidation
```

业务模块不再从 `session_id`、Canvas、grant 或 route 局部事实出发自行拼 `CapabilityState`。`CapabilityState` 是统一 projector 的输出，不是业务模块的写入 API。

收束方式不是把所有场景塞进一个无差别函数，而是建立一个统一的 AgentRun frame/surface command boundary。这个边界拥有上下文解析、写入 provenance、revision commit、live adoption 和 semantic delta emission；边界内部再按 command kind 分流到 construction 或 mutation。

AgentFrame 写入本身分为两类合法生产角色：

1. **Frame construction / launch commit**：创建或提交一个新的 AgentFrame surface revision，输入来自 owner bootstrap、ProjectAgent、companion、Lifecycle Workflow / AgentProcedure、accepted launch command。该类写入可以使用 `AgentFrameBuilder`，但必须收束在 frame construction service、lifecycle node composer 或 launch commit 边界内。
2. **Runtime surface update**：运行期 Canvas、WorkspaceModule、Permission、VFS/MCP/Skill inventory 等变化导致当前 AgentRun surface 需要更新。该类写入只能通过 `AgentRunRuntimeSurfaceUpdateService`。

Lifecycle Workflow / AgentProcedure 不属于运行期业务旁路。它是 frame construction 的 contract source：`AgentProcedureContract` 提供 capability、context、mount directive 和 hook rule 输入，由 lifecycle node composer 投影到 pending frame。

## 架构边界

### 统一服务

新增应用层 facade，建议归属 `agent_run::frame` 或 `agent_run` control-plane，而不是 Canvas / WorkspaceModule / Permission 模块。facade 对外表达一个入口、两类 typed command：

```rust
pub struct AgentRunFrameSurfaceService { ... }

pub enum AgentRunFrameSurfaceCommand {
    Construct(FrameConstructionCommand),
    Update(RuntimeSurfaceUpdateRequest),
}

pub enum FrameConstructionCommand {
    DispatchLaunchAnchor { run_id: Uuid, agent_id: Uuid, runtime_session_id: Option<String> },
    ComposeLaunchSurface { runtime_session_id: String, reason: LaunchComposeReason },
    CommitAcceptedLaunch { runtime_session_id: String, turn_id: String },
}
```

`Construct` 负责 AgentFrame 初始化、定义/contract 投影和 accepted launch commit；`Update` 负责运行期 surface mutation。这样 API 层和业务模块只看到一个 frame/surface command boundary，但内部不会把 construction 与 runtime mutation 混成同一个状态机。

运行期更新 request 示例：

```rust
pub enum RuntimeSurfaceUpdateRequest {
    CanvasBindingChanged { canvas_mount_id: String },
    CanvasVisibilityRequested { canvas_mount_id: String, reason: CanvasVisibilityReason },
    PermissionGrantApplied { grant_id: Uuid },
    PermissionGrantRevoked { grant_id: Uuid },
    McpPresetChanged { preset_key: String },
    ProjectVfsMountChanged { mount_id: String },
    WorkspaceModuleVisibilityChanged { module_ref: String },
    SkillInventoryChanged { provider_key: String },
}
```

MVP 不要求一次实现所有 variants，但类型要表达“业务只提交变化请求”。首批必须覆盖 Canvas binding/exposure 和 Permission surface-changing grant，因为这是当前已确认的散落写入路径。

并行保留 frame construction source 的清晰边界，而不是把 construction 也塞进 runtime update enum：

```rust
pub enum FrameConstructionSource {
    OwnerBootstrap,
    ProjectAgent,
    Companion,
    LifecycleAgentProcedure {
        orchestration_id: Uuid,
        node_path: String,
        attempt: u32,
    },
    AcceptedLaunchCommit,
}
```

实现上不一定需要立即落地该 enum，但代码结构和测试清单必须能表达同样的分类。`LifecycleDispatchService::materialize_workflow_agent_node` 可以继续负责 lifecycle agent / runtime session / anchor 的 materialization；AgentFrame surface 的细节必须委托 frame composer / construction service，不能在 dispatch 层扩张成第二套 surface projector。

### Entrypoint Ownership

最终目标的 ownership 分层：

| 层级 | 职责 | 可见性 |
| --- | --- | --- |
| `AgentRunFrameSurfaceService` | 唯一 application command boundary；解析 AgentRun context、分派 construction/update、提交 revision、触发 adoption/delta。 | Canvas、Permission、WorkspaceModule、Workflow/Lifecycle 等业务模块只依赖这一层或它暴露的 narrow use case。 |
| `FrameConstructionService` | construction 内部 composer；从 owner facts、AgentProcedure contract、ProjectAgent/companion facts 产出 launch envelope / pending frame。 | 只被 `AgentRunFrameSurfaceService`、session launch orchestration 或 lifecycle materialization adapter 调用。 |
| `AgentRunRuntimeSurfaceUpdateService` | update 内部 projector；从 typed update request replay capability/VFS/MCP/skill/workspace module surface。 | 只作为 frame surface facade 的内部 component 或同模块 sibling。 |
| `AgentFrameBuilder` | revision writer primitive；负责把 draft/surface 写成 immutable frame revision。 | 不作为业务模块 API；生产调用点必须在 construction/update/launch commit 白名单内。 |
| `adopt_persisted_agent_frame_revision` | live runtime sync primitive；重装 tool surface、active turn cache、hook runtime、ContextFrame emission。 | 不作为 route/tool/service 业务入口；只由 frame surface facade/update service 调用。 |

所以“同一入口”的定义是：业务语义只能进入 `AgentRunFrameSurfaceService` 的 typed command boundary；不是所有内部代码都共用同一个函数体，也不是让 Lifecycle dispatch、Canvas、Permission 各自继续持有 builder。

### Projection Context

统一服务内部解析 `AgentRunSurfaceProjectionContext`，包含：

- current `AgentFrameRuntimeTarget`
- current `AgentFrame`
- delivery runtime session id / active turn id
- `AuthIdentity`
- owner scope / subject / project id
- current typed VFS / MCP / capability surface
- runtime backend anchor
- skill discovery providers / extra skill dirs
- permission/admission projection inputs
- hook runtime target

这些字段不得由业务来源传入。业务来源只能传变化事实的最小稳定标识，例如 `canvas_mount_id` 或 `grant_id`。

### Projection Result

统一服务输出：

- `before_state`
- `after_state`
- typed semantic delta
- 是否写 AgentFrame revision
- 是否 immediate active-runtime adopt
- context frame / frontend invalidation metadata
- failure diagnostics

`adopt_persisted_agent_frame_revision` 可保留，但只作为服务内部 primitive。它继续负责 connector tool surface 重装、active turn cache 更新、hook runtime 对齐和 event emission。

## 数据流

### Canvas binding / visibility

旧链路：

```text
workspace_module_invoke(canvas.bind_data)
  -> bind_canvas_data_for_project
  -> refresh_canvas_mount_for_runtime
  -> expose_canvas_mount_revision_and_adopt
  -> AgentFrameBuilder::with_capability_state
  -> adopt_persisted_agent_frame_revision
```

新链路：

```text
workspace_module_invoke(canvas.bind_data)
  -> bind_canvas_data_for_project
  -> RuntimeSurfaceUpdateRequest::CanvasBindingChanged
  -> AgentRunRuntimeSurfaceUpdateService
```

`workspace_module_invoke` 只负责 operation dispatch 和 Canvas domain mutation，不再直接写 frame 或 adopt runtime。若 Canvas binding 变化只影响 provider 动态读取而不需要 model-visible surface revision，服务可以返回 no-op projection；若需要刷新 session-visible Canvas VFS / skill baseline，则由服务统一写 revision 并 adopt。

`workspace_module_present` 的 Canvas renderer 分支同理改为提交 `CanvasVisibilityRequested`。展示事件发送顺序由服务结果决定：需要 runtime exposure 时，先完成服务 projection/adoption，再发 `workspace_module_presented`。

### Permission grants

旧链路：

```text
PermissionGrantService
  -> compile RuntimeCapabilityTransition
  -> project_capability_state_from_frame
  -> AgentFrameBuilder::with_capability_state
API route
  -> adopt_persisted_agent_frame_revision
```

新链路：

```text
PermissionGrantService
  -> persist grant state / compile transition record
  -> RuntimeSurfaceUpdateRequest::PermissionGrantApplied|Revoked
  -> AgentRunRuntimeSurfaceUpdateService
  -> replay via capability dimension pipeline
```

Grant service 不再直接写完整 `CapabilityState`。API route 不再直接 adopt；它只调用 application service 并返回 structured result。tool-internal grant 继续只影响 AgentRun admission projection，不写 model-visible frame revision。

### Lifecycle Workflow / AgentProcedure

现有 workflow AgentCall materialization 的语义应保留：

```text
LifecycleRun.orchestration ready node
  -> LifecycleDispatchService::materialize_workflow_agent_node
  -> create LifecycleAgent / RuntimeSession / RuntimeSessionExecutionAnchor
  -> frame composer applies AgentProcedureContract
  -> AgentFrame construction revision
  -> reducer records NodeStarted
```

收束目标不是删除这条 construction 路径，而是防止它与 runtime surface update 混在一起：

- `LifecycleDispatchService` 负责控制面 materialization，不拥有 capability/VFS/MCP/context 投影细节。
- `composer_lifecycle_node` 负责从 orchestration anchor、plan node 和 `AgentProcedureContract` 组装 construction input，并调用统一 frame construction service 产出 pending frame。
- `AgentProcedureContract` 的 capability、mount directive、context、hook rule 贡献必须进入 frame construction surface draft，而不是在 lifecycle/workflow 模块内手写完整 `CapabilityState`。
- 如果未来支持已运行 AgentRun 的 AgentProcedure contract live update，它必须表达为 typed request，例如 `AgentProcedureContractChanged { run_id, agent_id, orchestration_id, node_path, attempt }`，并由 runtime surface update service 解析当前 AgentRun context 后投影。

这条边界使 AgentProcedure 成为“AgentFrame construction 的输入”，而不是“另一个可以写 AgentFrame 的业务模块”。

### Skill baseline

统一服务内部调用 skill baseline projection 时必须从 `AgentRunSurfaceProjectionContext` 传入 identity、active VFS、providers 和 owner/workspace facts。`derive_session_skill_baseline(SessionCapabilityProjectionInput { identity: None, ... })` 只能存在于测试或明确无身份的 frame construction 场景。

`merge_live_vfs_skill_entries` 改为 provider-aware：

- refreshed workspace/VFS skills 替换同 provider/capability key 的旧 workspace skills。
- non-workspace provider skills 默认保留，除非同 provider/capability key 在本次 projection 中有新结果覆盖。
- 不再通过 `file_path.contains("://")` 判断来源。

### Semantic delta

`capability_state_delta` 不能再作为所有 runtime surface 变化的用户语义。MVP 至少做到：

- `CapabilityKeyDimensionDelta::from_delta` 在 added/removed 为空时返回 `None`。
- VFS/Skill/MCP/WorkspaceModule 变化继续生成各自 section。
- 前端标题避免把纯 VFS/Skill update 展示为 `CAPABILITY DELTA`。

后续可将 frame kind 拆成 `runtime_surface_delta`、`capability_key_delta` 或按 section 推导标题；本任务内优先消除误导性空 CAP 卡和纯 surface 变化伪装。

## 冗余链路清理

以下链路必须迁移、删除或私有化：

| 旧入口 | 处理策略 |
| --- | --- |
| `WorkspaceModuleInvokeTool::refresh_canvas_mount_for_runtime` | 删除或改为提交 typed update request；不得调用 capability service expose/adopt。 |
| `SessionCapabilityService::expose_canvas_mount_revision_and_adopt` | 降级为统一服务内部 adapter 或删除。 |
| Canvas helper `expose_canvas_to_session` 直接依赖 `session_id` 并写 runtime surface | 改为返回/提交 exposure request；target/frame/context 由统一服务解析。 |
| `PermissionGrantService` 直接 `with_capability_state` 写 frame | 改为产出 transition/update request；统一服务 replay/write/adopt。 |
| API route `adopt_grant_effect` 直接 adopt | 删除 route-level adoption；调用 application service。 |
| 业务模块直接调用 `adopt_persisted_agent_frame_revision` | 只允许统一服务内部调用；收窄可见性或调整依赖注入。 |
| 业务路径手写 `SessionCapabilityProjectionInput` | 收束到 projection context；保留 owner bootstrap/frame construction 内部调用。 |
| Lifecycle / Workflow 模块绕过 frame composer 直接拼 runtime surface | 迁移为 construction source / composer input；dispatch 层只保留 agent/session/anchor materialization。 |
| AgentProcedure contract 变更直接写 current AgentFrame | 改为 construction request 或 runtime surface update request，由 AgentRun context 解析后投影。 |

## 兼容与迁移

项目未上线，不保留双路径 fallback。迁移时以测试锁住行为，阶段之间允许重命名和移动 API，只要每个阶段后编译和目标测试通过。

历史 session event 不做兼容迁移。前端只需正确展示新生成的 semantic frame；旧持久化 event 降级展示可接受。

## 风险

- `AgentFrame` 同时是 durable surface revision 和 live runtime cache anchor，迁移时必须保证写 frame、update connector tools、更新 hook runtime target 和 context frame emission 原子顺序一致。
- Permission grant 已经有领域状态机，迁移不能让 grant 状态成功但 surface update 静默失败。失败必须返回可见诊断。
- Canvas create/present 当前 spec 要求立即 session exposure；迁移后仍要保证 create/present 后 describe/invoke/present 的可见性语义明确。
- Workflow AgentCall 当前要求 agent/frame/session/anchor materialization 与 runtime node `NodeStarted` 一致。收束写入边界时不能让 AgentFrame construction 成功但 orchestration reducer 未记录启动，或反向出现 runtime node running 但缺少有效 frame。
- 前端刷新逻辑目前依赖 `capability_state_delta` 做粗粒度 invalidation；拆 semantic frame 时要保留必要刷新触发。

## 测试策略

- Rust 单元测试：`CapabilityKeyDimensionDelta` 空 delta 不生成 section。
- Rust 集成/应用测试：external integration skill 初始可见，Canvas binding/visibility update 后仍可见。
- Rust 应用测试：`workspace_module_invoke(canvas.bind_data)` 不直接调用旧 expose/adopt 链路；通过统一 update service 处理 runtime surface。
- Rust 应用测试：workflow AgentCall materialization 仍通过 lifecycle dispatch 创建 agent/frame/session/anchor，并通过 frame composer 应用 AgentProcedure contract；静态检查确认该路径是 frame construction 而非 runtime surface update 旁路。
- Permission 测试：approve/revoke surface-changing grant 通过统一 service 写/adopt，route 不直接调用 adoption primitive。
- 前端测试：纯 VFS/Skill semantic update 不显示 `Capability Keys no change`，标题不误报为 capability key delta。

## 规划结论

本任务不是修补 `identity: None`，而是把 runtime surface 更新从业务模块手中收回到 AgentRun control-plane。冗余链路清理是验收的一部分：只要旧旁路仍可被业务直接调用，未来还会重新出现上下文漏传和虚假 delta。
