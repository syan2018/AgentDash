# Research: AgentRun lifecycle 与 builtin SkillAsset projection 收束路径

- Query: 围绕 AgentRun lifecycle 与 builtin SkillAsset projection 收束，定位当前代码路径、必须修改的函数、已有测试和风险点。
- Scope: internal
- Date: 2026-06-15

## Findings

### Files found

- `.trellis/workflow.md` — Trellis 阶段与 research artifact 持久化约束；当前 `task.py current --source` 返回无 active task，本文件按用户显式指定路径写入。
- `.trellis/spec/backend/workflow/architecture.md` — 规定 `AgentFrame` 是 capability/context/VFS/MCP 的 runtime surface，AgentRun workspace resource surface 从 current frame typed VFS 投影。
- `.trellis/spec/backend/workflow/activity-lifecycle.md` — 规定 workflow runtime 从 `LifecycleRun.orchestrations[]` 派生，runtime node 以 `orchestration_id + node_path + attempt` 定位。
- `.trellis/spec/backend/vfs/architecture.md` — 规定 AgentRun workspace 使用 `RuntimeSessionExecutionAnchor` 叠加 `agent_run_session` lifecycle mount，ProjectAgent/Workflow AgentCall 执行期写入 `node_runtime` lifecycle mount。
- `.trellis/spec/backend/vfs/vfs-access.md` — 定义 `agent_run_session` 与 `node_runtime` 两种 lifecycle mount contract，以及 SkillAsset 文件通过 `skills/<key>/...` 投影。
- `.trellis/spec/backend/session/session-startup-pipeline.md` — 规定 `FrameConstructionService` 通过 anchor/current frame 生成 `FrameLaunchEnvelope`，业务模块不得绕过 frame construction 自行组装 connector facts。
- `crates/agentdash-application/src/workflow/lifecycle/mount.rs` — workflow/lifecycle 领域到 VFS mount 的 projection helper，包含 active workflow `node_runtime` mount 和 AgentRun workspace `agent_run_session` mount。
- `crates/agentdash-application/src/vfs/mount.rs` — runtime mount builder，定义 `build_agent_run_session_lifecycle_mount`、`build_lifecycle_mount_with_node_scope`、`append_lifecycle_skill_asset_projection`。
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs` — `lifecycle_vfs` provider，同时服务 `agent_run_session` 只读证据面、`node_runtime` artifact/record 面，以及 metadata 驱动的 SkillAsset projection。
- `crates/agentdash-application/src/vfs/provider_skill_asset.rs` — SkillAsset projection 的通用读取/列举/搜索实现，`lifecycle_vfs` 复用这些函数。
- `crates/agentdash-application/src/companion/skill_projection.rs` — companion system builtin SkillAsset 的 ensure/key append/lifecycle metadata projection helper。
- `crates/agentdash-application/src/workflow/frame_construction/owner_bootstrap.rs` — Project/Story owner frame bootstrap；当 active workflow 产生 lifecycle mount 时投影 companion system skill metadata。
- `crates/agentdash-application/src/workflow/frame_construction/composer_companion.rs` — companion frame construction 入口，带 workflow 时转入 `compose_companion_with_workflow_to_frame`。
- `crates/agentdash-application/src/workflow/frame_construction/composer_lifecycle_node.rs` — lifecycle node frame construction 入口，从 `RuntimeSessionExecutionAnchor` 取 orchestration/node/attempt，再 compose lifecycle node frame。
- `crates/agentdash-application/src/session/assembler.rs` — lifecycle node 与 companion+workflow 的共享 assembly 路径。
- `crates/agentdash-application/src/session/assembly_builder.rs` — `SessionAssemblyBuilder` 把 VFS/capability/MCP/context 收束为 `FrameSurfaceDraft` 并写入 `AgentFrameBuilder`。
- `crates/agentdash-application/src/workflow/agent_run_workspace/query.rs` — AgentRun workspace snapshot 查询，从 current/anchor frame typed VFS 叠加 `agent_run_session` lifecycle mount 后构建 resource surface。

### Current code path

1. Active workflow / owner bootstrap path:
   `OwnerBootstrapComposer::compose_owner_bootstrap` 调 `prepare_owner_bootstrap_vfs`，先通过 `project_active_workflow_lifecycle_vfs` 追加/替换 `lifecycle` mount，再发现存在 lifecycle mount 时确保 companion system builtin SkillAsset 并追加 key，最后调用 `append_lifecycle_skill_asset_projection` 写入 mount metadata（`owner_bootstrap.rs:207`, `owner_bootstrap.rs:316`, `owner_bootstrap.rs:367`, `owner_bootstrap.rs:370`, `owner_bootstrap.rs:371`, `owner_bootstrap.rs:384`）。

2. Lifecycle node compose path:
   `composer_lifecycle_node::compose` 从 `RuntimeSessionExecutionAnchor` 解析 `orchestration_id`、`node_path`、`attempt`，找到 plan node 和 workflow contract 后调用 `compose_lifecycle_node_to_frame_with_audit`（`composer_lifecycle_node.rs:27`, `composer_lifecycle_node.rs:39`, `composer_lifecycle_node.rs:45`, `composer_lifecycle_node.rs:51`, `composer_lifecycle_node.rs:62`, `composer_lifecycle_node.rs:94`）。
   `compose_lifecycle_node_with_audit` 调 `activate_activity_with_platform` 生成 `node_runtime` lifecycle VFS，然后调用 `project_companion_system_skill_to_activation` 把 companion system SkillAsset metadata 写入 activation 的 lifecycle mount/VFS（`assembler.rs:302`, `assembler.rs:323`, `assembler.rs:343`）。

3. Companion + workflow path:
   `composer_companion::compose` 在 `companion.workflow` 存在时调用 `compose_companion_with_workflow_to_frame`（`composer_companion.rs:24`, `composer_companion.rs:26`）。
   `compose_companion_with_workflow` 先从 parent VFS/MCP 生成 companion slice，再调用 `activate_activity_with_platform`，最后通过 `SessionAssemblyBuilder::with_vfs(slice.vfs).apply_lifecycle_activation(&activation, ...)` 把 lifecycle activation 叠加到 child frame（`assembler.rs:594`, `assembler.rs:605`, `assembler.rs:620`, `assembler.rs:679`, `assembler.rs:683`）。
   这里当前没有调用 `project_companion_system_skill_to_activation`，因此 companion+workflow child 的 lifecycle mount 可能缺少 builtin companion system SkillAsset metadata。

4. Activity activation / frame surface path:
   `activate_activity_with_platform` 用 `build_lifecycle_mount_with_node_scope` 创建 `node_runtime` lifecycle mount，metadata 包含 `run_id`、`orchestration_id`、`node_path`、`lifecycle_key`、`scope=node_runtime`、`writable_port_keys`、`attempt`（`activity_activation.rs:111`, `activity_activation.rs:139`, `activity_activation.rs:147`；builder 细节在 `mount.rs:943`）。
   `build_lifecycle_activation_surface` 把 base VFS、activation lifecycle VFS 和 mount directives 合并，并写回 `CapabilityState.vfs.active`（`frame_builder.rs:52`, `frame_builder.rs:55`, `frame_builder.rs:60`）。
   `SessionAssemblyBuilder::apply_lifecycle_activation` 使用该 surface 覆盖 builder 的 VFS/capability/MCP/input/executor（`assembly_builder.rs:326`, `assembly_builder.rs:331`, `assembly_builder.rs:339`）。
   `project_assembly_to_frame` 将 `FrameSurfaceDraft` 写入 `AgentFrameBuilder`（`assembly_builder.rs:412`, `assembly_builder.rs:419`, `assembly_builder.rs:420`）。

5. AgentRun workspace resource surface path:
   `AgentRunWorkspaceQueryService::resolve` 调 `resolve_agent_run_frame_vfs` 获取 frame + VFS，再通过 `build_surface_summary` 构建 resource surface（`query.rs:57`, `query.rs:70`, `query.rs:78`, `query.rs:85`）。
   `resolve_agent_run_frame_vfs` 优先取 current frame，若无 current frame 则退到 anchor launch frame；存在 anchor 时调用 `build_agent_run_lifecycle_vfs(frame.typed_vfs(), anchor)`（`query.rs:254`, `query.rs:267`, `query.rs:268`, `query.rs:277`）。
   `build_agent_run_lifecycle_vfs` 调 `install_agent_run_lifecycle_mount`，后者先删除所有 id 为 `lifecycle` 的 mount，再安装 `build_agent_run_session_lifecycle_mount` 生成的 `scope=agent_run_session` mount（`mount.rs:94`, `mount.rs:95`, `mount.rs:96`, `mount.rs:108`, `mount.rs:113`）。
   这会把 frame typed VFS 中原 `node_runtime` lifecycle mount 上的 `skill_asset_project_id` / `skill_asset_keys` metadata 一并丢弃。

6. Provider behavior:
   `LifecycleMountProvider::read_text` / `list` / `search_text` 在处理 scope 前先检查 `parse_skill_asset_mount_metadata(mount).is_ok()`；metadata 存在时，`skills/...` 路径会直接复用 `read_projected_skill_file` / `list_projected_skill_files` / `search_projected_skill_files`（`provider_lifecycle.rs:533`, `provider_lifecycle.rs:544`, `provider_lifecycle.rs:696`, `provider_lifecycle.rs:705`, `provider_lifecycle.rs:809`, `provider_lifecycle.rs:820`）。
   因此 provider 已支持在 `agent_run_session` 或 `node_runtime` mount 上叠加 SkillAsset projection，问题集中在 mount metadata 是否持续传递。

### Must-modify functions

- `crates/agentdash-application/src/session/assembler.rs:594` `compose_companion_with_workflow`
  - 当前创建 `activation` 后直接 `apply_lifecycle_activation`，缺少 `project_companion_system_skill_to_activation`。
  - 建议改为 `let mut activation = ...; project_companion_system_skill_to_activation(repos, project_id, &mut activation).await?;`，与 `compose_lifecycle_node_with_audit` 对齐。

- `crates/agentdash-application/src/workflow/lifecycle/mount.rs:94` `install_agent_run_lifecycle_mount`
  - 当前用 session-scoped lifecycle mount 替换原 lifecycle mount，未保留原 mount 上的 SkillAsset metadata。
  - 建议在 retain 前读取旧 lifecycle mount 的 `skill_asset_project_id` / `skill_asset_keys`，构建 `agent_run_session` mount 后重新追加这两个 metadata，或抽一个 helper 专门复制 SkillAsset projection metadata。

- `crates/agentdash-application/src/workflow/lifecycle/mount.rs:108` `build_agent_run_lifecycle_vfs`
  - 如果不在 `install_agent_run_lifecycle_mount` 内处理，这里是另一个可改点；它拥有 base VFS 和 anchor，能在安装 session mount 前后做 projection metadata carry-over。
  - 更推荐改 `install_agent_run_lifecycle_mount`，因为调用者只需给出 VFS + anchor，不需要知道 metadata 细节。

- `crates/agentdash-application/src/vfs/mount.rs:835` `build_agent_run_session_lifecycle_mount`
  - 若希望 builder 本身支持 builtin SkillAsset projection，可扩展输入或新增 wrapper；但直接改签名会影响调用点。较低扰动做法是保持 builder 只负责基础 mount，在 `install_agent_run_lifecycle_mount` 上层补 metadata。

- `crates/agentdash-application/src/vfs/mount.rs:1010` `append_lifecycle_skill_asset_projection`
  - 该函数已能写入 lifecycle mount metadata，但会用新 keys 覆盖旧 keys；若需要“保留原 keys + 追加 builtin keys”的语义，应增加 merge helper 或调整调用方先合并 keys。

### Existing tests

- `workflow/lifecycle/mount.rs`
  - `active_workflow_projection_creates_vfs_when_base_is_absent` 覆盖 active workflow 创建 `node_runtime` lifecycle mount（`mount.rs:226`）。
  - `active_workflow_projection_preserves_existing_mounts_and_replaces_stale_lifecycle` 覆盖替换 stale lifecycle mount 并保留 main mount（`mount.rs:255`）。
  - `agent_run_lifecycle_vfs_installs_session_scoped_mount` 覆盖 AgentRun workspace 安装 `agent_run_session` mount（`mount.rs:304`）。
  - `agent_run_lifecycle_vfs_replaces_stale_node_scoped_mount` 覆盖 session mount 替换 node-scoped mount，但当前只断言 scope/node/attempt，没有断言 SkillAsset metadata carry-over（`mount.rs:339`）。
  - `agent_run_lifecycle_vfs_uses_lifecycle_default_when_base_is_absent` 覆盖无 base VFS 时 default mount（`mount.rs:385`）。

- `companion/skill_projection.rs`
  - `companion_system_key_is_appended_once` 覆盖 key 去重（`skill_projection.rs:98`）。
  - `lifecycle_projection_writes_companion_system_to_mount_metadata` 覆盖 companion system projection 写入 lifecycle mount metadata（`skill_projection.rs:107`）。

- `session/assembler.rs`
  - `apply_lifecycle_activation_merges_existing_vfs` 覆盖 lifecycle activation 与已有 VFS 合并（`assembler.rs:894`）。
  - `lifecycle_context_contribution_contains_workflow_and_runtime_fragments` 覆盖 lifecycle context/runtime policy fragment（`assembler.rs:921`）。
  - 未看到针对 `compose_companion_with_workflow` 的 SkillAsset projection 测试。

- `vfs/provider_lifecycle.rs`
  - `agent_run_session_mount_lists_graphless_session_log_surface` 覆盖 graphless AgentRun session log surface（`provider_lifecycle.rs:1260`）。
  - `agent_run_session_mount_exposes_anchor_node_without_project_wide_orchestration` 覆盖带 node anchor 的 AgentRun session surface（`provider_lifecycle.rs:1302`）。
  - `agent_run_session_mount_rejects_direct_writes` 覆盖 session scope 只读（`provider_lifecycle.rs:1339`）。
  - `node_runtime_mount_exposes_only_current_node_writable_surface` 覆盖 node runtime 写入白名单（`provider_lifecycle.rs:1358`）。
  - 未看到 lifecycle_vfs + SkillAsset metadata 的 provider 集成测试；SkillAsset projection 本身在 `provider_skill_asset.rs` 有独立测试。

- `vfs/provider_skill_asset.rs`
  - `skill_asset_projection_is_discoverable_by_existing_skill_loader` 覆盖 `skills/<key>/...` projection 与现有 skill loader 兼容（`provider_skill_asset.rs:737`）。
  - `skill_asset_mount_lists_only_selected_keys` 覆盖 selected keys 过滤（`provider_skill_asset.rs:767`）。
  - `skill_asset_mount_exposes_binary_metadata_and_skips_text_read_search`、`skill_asset_mount_reads_binary_file` 覆盖 binary metadata/read 行为（`provider_skill_asset.rs:792`, `provider_skill_asset.rs:862`）。

### Risks / implementation notes

- SkillAsset metadata 丢失风险最高：AgentRun workspace 查询会把 current frame 的 `node_runtime` lifecycle mount替换为 `agent_run_session` mount；如果不 carry-over `skill_asset_project_id` / `skill_asset_keys`，workspace resource browser 无法在 session-scoped lifecycle mount 下看到 builtin skill files。
- Companion+workflow 路径与 lifecycle node 路径不一致：`compose_lifecycle_node_with_audit` 已投影 companion system builtin SkillAsset，但 `compose_companion_with_workflow` 未投影。若目标是 companion/lifecycle convergence，这条路径需要补齐。
- Provider 已经按 metadata 驱动 projection；不要把 SkillAsset 文件路径硬编码到 `provider_lifecycle.rs` 的 `agent_run_session` / `node_runtime` match 中，否则会把通用 projection 分散。
- `append_lifecycle_skill_asset_projection` 当前在 keys 非空时覆盖 metadata；若一个 lifecycle mount 同时来自 agent preset skill keys、companion system skill、workspace module system skill，需要调用方先合并去重，或新增公共 merge helper。
- `agent_run_session` 是只读证据面；保留 SkillAsset projection metadata 不应开放写能力。`build_agent_run_session_lifecycle_mount` 当前 capabilities 是 Read/List/Search，这与 spec 一致。
- `build_agent_run_lifecycle_vfs(None, anchor)` 没有 base VFS，因此无法推导 SkillAsset keys；这种情况下只能得到基础 lifecycle session evidence surface。

### Suggested focused test additions

- 在 `workflow/lifecycle/mount.rs` 增加测试：base VFS 的 stale `node_runtime` lifecycle mount 带 `skill_asset_project_id` / `skill_asset_keys`，调用 `build_agent_run_lifecycle_vfs` 后新的 `agent_run_session` mount 保留这些 metadata。
- 在 `session/assembler.rs` 增加测试或小型集成测试：`compose_companion_with_workflow` 生成的 prepared VFS 中 lifecycle mount 带 companion system SkillAsset key。
- 在 `vfs/provider_lifecycle.rs` 增加 provider 测试：带 SkillAsset metadata 的 `agent_run_session` mount 能 list/read `skills/<companion_system>/SKILL.md`，同时仍拒绝普通 write。

## Caveats / Not Found

- 未运行测试；本次仅做 research，不修改生产代码。
- 当前 Trellis active task 未设置，`python ./.trellis/scripts/task.py current --source` 返回 `(none)`；研究文件写入用户明确指定的 task research 路径。
- 未发现 `AgentRunWorkspaceQueryService::resolve_agent_run_frame_vfs` 的直接单元测试；相关行为主要由 lifecycle mount helper、conversation snapshot diagnostic 和 provider tests 间接覆盖。
- 未检索外部资料；本问题完全基于本地代码与 `.trellis/spec/`。
