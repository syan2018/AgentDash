# WI-07 AgentFrame ContextDelivery

## Objective

重建 AgentFrame 内部 surface 和 ContextDelivery 输入事实，使能力与认知状态只锚定在 AgentFrame revision，而 ContextFrame emission 只来自 accepted input fact。

## Decisions

D-011, D-012, D-014, D-017

## Research Inputs

- `research/agentframe-context-surface.md`
- `research/database-physical-design.md`

## Scope

- 设计 AgentFrame canonical surface：capability、VFS、MCP、executor、visible workspace refs、context surface。
- 删除 historical frame revision 原地 append visible refs 的路径。
- 删除 capability/VFS/MCP 多列覆盖式双源写入。
- 明确 current/applied frame binding，不再用 highest revision 直接代表 runtime current truth。
- 引入 `ContextDeliveryRecord` 或等价 accepted input fact。
- 让 ContextFrame emission、connector input、runtime turn、applied frame 可互相追溯。

## Out Of Scope

- accepted boundary 集成交给 WI-05。
- current delivery selection 交给 WI-06。
- contracts/frontend projection 展示交给 WI-09。

## Dependencies

依赖 WI-00 inventory 和 WI-06 delivery binding 方向。

## Implementation Notes

- `AgentFrameRepository` 可以保留为 revision surface store。
- 细粒度 mutation helper 应改为 frame aggregate update 或 frame surface command port。
- 如果为了查询保留物理列，应作为 generated projection 或只读缓存，不成为独立写源。

## Acceptance

- AgentFrame revision append-only，历史 revision 不被 runtime path 原地修改。
- Agent capability / cognition 判断只从 AgentFrame effective surface 得出。
- ContextFrame 不再由 launch、commit、transition、compaction 多处独立构造。
- applied frame 和 launch frame 的关系可审计。

## Validation

- frame builder / repository roundtrip 测试覆盖 append-only revision。
- ContextDeliveryRecord 到 ContextFrame emission 的映射测试。
- `rg "append_visible|get_current\\(agent_id\\)"` 清点并替换不符合目标的路径。

## Implementation Record - 2026-07-05 C2

- 删除 `AgentFrame::append_visible_canvas_mount` / `append_visible_workspace_module_ref` 域对象可变入口；运行期 canvas exposure 只在未提交的新 `AgentFrame` revision 上物化 visible projection。
- 将 workspace canvas VFS 投影函数改名为 `project_visible_canvas_mounts`，让调用点表达“按 accepted visible ids 重建投影”，而不是写入历史 frame。
- `project_capability_state_from_frame` 以 `effective_capability_json` 作为 canonical capability surface；split VFS/MCP 列只在 canonical 维度缺失时补全，不再覆盖 canonical state。
- 新增 SPI `ContextDeliveryRecord`，在 `TurnPreparer` 中用 launch frame surface、pending frame、delivery plan、connector target 和实际 emission ids 生成 accepted input fact。
- `TurnCommitter` 在 `context_frame` emission 前持久化 `context_delivery_record` session meta event，连接 connector input、runtime turn、applied frame surface 和 ContextFrame emission。

Validation run:

- `cargo test -p agentdash-domain agent_frame --lib`
- `cargo test -p agentdash-application-agentrun build_revision --lib`
- `cargo test -p agentdash-application-agentrun canvas_expose --lib`
- `cargo test -p agentdash-application-agentrun runtime_surface_noop_compare_uses_frame_surface_not_revision_identity --lib`
- `cargo test -p agentdash-application-agentrun effective_view_projects_selected_frame_surface --lib`
- `cargo test -p agentdash-application-agentrun product_port_effective_capability_projects_visible_state_without_grant_mutation --lib`
- `cargo test -p agentdash-application-agentrun project_round_trip_preserves_capability_state --lib`
- `cargo test -p agentdash-application-agentrun effective_capability_json_remains_canonical_when_split_vfs_differs --lib`
- `cargo test -p agentdash-application-runtime-session context_delivery_record_links_connector_turn_frame_and_emissions --lib`
- `cargo test -p agentdash-application-runtime-session context_delivery_record_envelope_uses_record_key_and_turn_trace --lib`
- `cargo test -p agentdash-application-runtime-session delivery_plan_orders_frames_and_keeps_pi_memory_out_of_system --lib`
- `cargo fmt --check`
- `cargo check -p agentdash-domain -p agentdash-infrastructure -p agentdash-application-runtime-session -p agentdash-application-agentrun -p agentdash-application -p agentdash-api`
- `rg -n "append_visible|get_current\\(agent_id\\)|effective_capability_json|vfs_surface_json|mcp_surface_json" crates`
- `git diff --check`

Remaining boundary for WI-05 / C1:

- `append_visible` 已无 crates 命中；验收 grep 仍列出 split physical surface columns，后续 schema/canonical surface slice 需要把这些列降为生成投影或移出写源。
- `AgentRunEffectiveCapabilityAdapter` 与 permission/companion 读路径仍存在 `get_current(agent_id)`；当前 C2 保持不改 current delivery binding/anchor 文件，后续 accepted boundary 应消费 C1 的 current delivery binding API 来定位 applied frame。
- `context_compacted` 的直接 compaction frame 投影仍由 eventing 派生，它已不参与本次 turn accepted input record；后续 ContextFrame 全量收口需要把 compaction emission 纳入同类 accepted fact。
