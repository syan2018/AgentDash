# Capability Exposure Design

## Design Decisions To Preserve

- AgentRun 是 runtime capability / exposure 访问的唯一入口。能力事实源从 AgentRun 运行身份向下解析到当前 `LifecycleAgent`、`AgentFrame` revision、runtime policy 与运行期护栏投影。
- AgentFrame 是 AgentRun model-visible runtime surface 的 revision/snapshot，承载 procedure、capability、context slice、VFS、MCP、workspace module visibility 等可恢复能力上下文。
- PermissionGrant 是 AgentRun-scoped 独立授权/护栏系统，负责审批、审计、生命周期、工具内部能力准入，以及为 Agent 直接拓展工具集的请求。Grant 不被业务模块直接访问或解析；它只由 AgentRun effective capability/admission 服务消费并投影成最终能力视图或执行准入结果。
- Canvas / WorkspaceModule / hook runtime / VFS surface 都必须经 AgentRun effective runtime capability view 派生或刷新，不能从 live cache、PermissionGrant 或 route-local helper 各自拼接。

## Target Flow

```text
Command intent
  -> AgentRun effective runtime capability service
  -> select current run/agent/frame
  -> write AgentFrame revision only when model-visible runtime surface changes
  -> project runtime guardrails and grant effects from AgentRun-scoped ledgers
  -> derive live VFS, WorkspaceModule visibility, hook runtime surface from AgentRun view
  -> diagnostics records derivation failures
```

## Fact Model Decision

- model-visible runtime exposure / capability 变更通过新的 `AgentFrame` revision 表达。
- `AgentFrame` revision 是 AgentRun 下可更新可替换 runtime ability state 的事实锚点；每个 revision 必须能恢复 effective capability、VFS、MCP 与 workspace module visibility。
- 不使用独立 exposure table 作为第一版事实源，原因是会引入第二套 owner/repository。
- 不原地更新当前 frame exposure 字段，原因是 revision history 将无法解释能力状态变化。
- PermissionGrant 不默认产生 AgentFrame revision。Grant 可以表达两类效果：工具内部能力准入，以及为 AgentRun 拓展可见工具集。前者保留在 AgentRun admission projection 中，后者通过 AgentRun capability service 写入新的 AgentFrame revision。
- 任何模块看到的最终可见能力都来自 AgentRun effective capability view。Grant ledger、AgentFrame、runtime policy 的组合关系只存在于 AgentRun 服务内部。

## Exposure Fact Shape

AgentFrame exposure fact reuses existing columns rather than adding an exposure table:

- `effective_capability_json`: effective runtime `CapabilityState`.
- `vfs_surface_json`: VFS surface derived from that capability state.
- `mcp_surface_json`: MCP surface derived from that capability state.
- `visible_canvas_mount_ids_json`: Canvas mount exposure refs.
- `visible_workspace_module_refs_json`: runtime-visible workspace module refs such as `canvas:{mount_id}`.
- `created_by_kind` / `created_by_id`: command source such as `canvas_expose`, `workspace_module_present`, or a grant-derived surface change that explicitly changes model-visible runtime exposure.

Canvas exposure and WorkspaceModule runtime visibility write a new AgentFrame revision first. PermissionGrant only writes a revision through this path when the approved/revoked/expired effect changes AgentRun model-visible exposure, such as adding or removing a model-visible tool capability. Tool-internal admission stays in the AgentRun guardrail projection. Any live runtime cache adoption happens after the persisted revision exists.

## AgentRun Effective Capability Boundary

All runtime capability reads should pass through a single AgentRun-level resolver/service. The service owns:

- current AgentRun coordinate selection: run, agent, current frame and delivery runtime;
- model-visible capability state from the selected AgentFrame revision;
- runtime guardrail projection from AgentRun-scoped ledgers such as PermissionGrant, command policy and runtime admission state;
- Grant effect classification: tool-internal admission remains an admission decision, while Agent toolset expansion becomes an AgentFrame surface revision;
- derived live surfaces for VFS, MCP, WorkspaceModule visibility, hook runtime and tool admission.

Consumers receive either final visible capability from AgentRun or an AgentRun admission decision. Direct `AgentFrame + PermissionGrant` composition, active-grant reads in `CapabilityResolver`, live VFS-derived visibility and route-local helper decisions are replaced by this boundary.

## Recovery Rule

On recovery, live VFS / WorkspaceModule visibility / hook runtime state must be reconstructed from AgentFrame exposure fact. Runtime caches may fail independently but cannot become the source of truth.

Recovery order:

1. Resolve AgentRun to the current delivery binding and current AgentFrame.
2. Read effective capability/VFS/MCP and visible runtime refs from that frame revision.
3. Build AgentRun effective runtime capability view by layering runtime guardrail/admission projection without mutating the frame fact.
4. Reconstruct live VFS and skill baseline from the AgentRun view.
5. Resolve WorkspaceModule visibility from base capability dimension plus frame runtime refs.
6. Rebuild hook runtime for the frame target and enqueue context/tool deltas.
7. Emit presentation/runtime events only after frame fact write and runtime adoption succeed.

## Implementation Boundary

CE05 is a prerequisite boundary check: AgentRun effective capability/admission is the only path that may consume Grant state. CE02 defines Grant classification inside AgentRun: tool-internal permission becomes admission projection; Agent toolset expansion becomes model-visible surface revision through AgentFrame. CE03/CE04 consume the AgentRun capability view as the only access path.

Replacement cleanup after CE05-CE04: active-grant resolver input, row-update exposure append APIs as production writers, live VFS-first Canvas exposure, and WorkspaceModule local visibility resolution all fold into the AgentRun boundary.
