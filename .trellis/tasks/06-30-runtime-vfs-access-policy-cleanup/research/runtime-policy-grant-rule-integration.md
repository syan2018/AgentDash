# Research: RuntimeVfsAccessPolicy typed PermissionGrant rule integration

- Query: 只读研究现有 `RuntimeVfsAccessPolicy` carrier/compiler/enforcement 如何接入 typed `PermissionGrant` VFS rules，并收束 provider mount capability、`CapabilityState.vfs` 与 runtime admission 的事实源边界。
- Scope: internal
- Date: 2026-06-30

## Findings

### Files found

- `.trellis/tasks/06-30-runtime-vfs-access-policy-cleanup/prd.md` - 当前任务要求把运行期 VFS 授权收束到 `RuntimeVfsAccessPolicy`，并明确 typed `PermissionGrant` VFS path rules 尚未完成。
- `.trellis/tasks/06-30-runtime-vfs-access-policy-cleanup/design.md` - 设计边界把 tool visibility、provider capability、runtime VFS admission 分离，且明确不要从当前 `ToolCapabilityPath` 字符串伪造 VFS path grant。
- `.trellis/tasks/06-30-runtime-vfs-access-policy-cleanup/implement.md` - 已实现 whole-mount policy/carrier/enforcement，后续缺口是 typed `PermissionGrant` VFS path-level rules。
- `.trellis/spec/backend/vfs/vfs-access.md` - 规范要求 VFS 地址使用 `surface_ref + mount_id + mount_relative_path`，ProjectAgent preset 只表达 Project VFS mount exposure，通用 mount/path admission 由 `RuntimeVfsAccessPolicy` 表达。
- `.trellis/spec/backend/vfs/vfs-materialization.md` - 规范要求 VFS URI 物化前检查 source mount read capability，且物化路径不是 VFS 写入口。
- `.trellis/spec/backend/session/execution-context-frames.md` - 规范定义 `ExecutionContext.session` 是 connector-facing session projection；当前文本还只列出 `vfs: Option<Vfs>`，需要与代码里的 `vfs_access_policy` 对齐。
- `crates/agentdash-spi/src/connector/mod.rs` - `ExecutionSessionFrame` 和 `RuntimeVfsAccessPolicy` SPI carrier/type 定义。
- `crates/agentdash-application-vfs/src/access_policy.rs` - whole-mount compiler wrapper、matcher wrapper、policy tests。
- `crates/agentdash-application-vfs/src/service.rs` - provider dispatch、apply_patch、exec、search 等 normalized path 后的 policy enforcement。
- `crates/agentdash-application-vfs/src/materialization.rs` - shell command / relay MCP JSON argument materialization policy enforcement。
- `crates/agentdash-application-vfs/src/tools/common.rs` - `SharedRuntimeVfs` 同时携带 `Vfs` 与 `RuntimeVfsAccessPolicy`。
- `crates/agentdash-application/src/runtime_tools/provider.rs` - 从 `ExecutionContext.session.vfs_access_policy` 构造 runtime VFS tools。
- `crates/agentdash-application-runtime-session/src/session/launch/plan.rs` - launch plan 目前从 `Vfs` whole-mount 编译 session `vfs_access_policy`。
- `crates/agentdash-application-runtime-session/src/session/tool_assembly.rs` - relay MCP call context 传递 `vfs_access_policy`。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs` - AgentRun runtime surface query 从 frame typed VFS whole-mount 编译 policy。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs` - runtime surface update test helper 同样 whole-mount 编译 policy。
- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs` - 当前 active `PermissionGrant` 只投影 tool-level admission，不产生 VFS rules。
- `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs` - approve/revoke 后只按 capability/tool path 更新 `CapabilityState.tool` 和 AgentFrame surface。
- `crates/agentdash-application/src/permission/compiler.rs` - `PermissionGrantCompiler` 只生成 tool dimension `CapabilityDirective`。
- `crates/agentdash-domain/src/permission/entity.rs` - `PermissionGrant.requested_paths` 当前类型是 `Vec<ToolCapabilityPath>`。
- `crates/agentdash-domain/src/workflow/value_objects/capability.rs` - `ToolCapabilityPath` 只表达 `"cap"` 或 `"cap::tool"`，不是 mount/path/operation rule。
- `crates/agentdash-domain/src/common/agent_config.rs` - ProjectAgent preset 字段已收窄为 `project_vfs_mount_exposure_grants`，旧 `vfs_access_grants` 被拒绝。
- `crates/agentdash-application-vfs/src/mount_project.rs` - Project VFS mount exposure 只裁剪 Project VFS mount capabilities，不约束 agent memory 等 runtime mounts。
- `crates/agentdash-application/src/frame_construction/request_assembler.rs`、`composer_project_agent.rs`、`owner_bootstrap.rs` - ProjectAgent preset exposure 在 frame construction / owner bootstrap 阶段作用于 VFS composition。

### Current RuntimeVfsAccessPolicy type and whole-mount compiler

- `ExecutionSessionFrame` 已把 policy 作为 session projection 字段带给 connector：`vfs: Option<Vfs>` 后面是 `vfs_access_policy: Option<RuntimeVfsAccessPolicy>`，位置在 `crates/agentdash-spi/src/connector/mod.rs:75` 和 `crates/agentdash-spi/src/connector/mod.rs:76`。
- SPI policy type 已包含 operation/path/source：`RuntimeVfsOperation` 包含 `Read/List/Search/Write/Exec/ApplyPatch`（`crates/agentdash-spi/src/connector/mod.rs:98`），`RuntimeVfsPathPattern` 当前为 `All` / `Prefix(String)`（`crates/agentdash-spi/src/connector/mod.rs:109`），`RuntimeVfsAccessSource` 已预留 `ProjectPreset`、`PermissionGrant`、`SystemRuntimeProjection`（`crates/agentdash-spi/src/connector/mod.rs:131`）。
- `RuntimeVfsAccessRule` 是 `mount_id + path_pattern + operations + source`，定义在 `crates/agentdash-spi/src/connector/mod.rs:139` 到 `crates/agentdash-spi/src/connector/mod.rs:145`；`RuntimeVfsAccessPolicy` 是 rule vec，定义在 `crates/agentdash-spi/src/connector/mod.rs:147` 到 `crates/agentdash-spi/src/connector/mod.rs:149`。
- whole-mount compiler 目前直接从 `Vfs.mounts` 生成 `RuntimeVfsPathPattern::All` rule，并以 mount capability 映射 operation set，见 `RuntimeVfsAccessPolicy::whole_mounts_from_vfs_with_source` 的 `filter_map` 和 `RuntimeVfsAccessRule` 构造，`crates/agentdash-spi/src/connector/mod.rs:160` 到 `crates/agentdash-spi/src/connector/mod.rs:177`。
- mount capability 到 runtime operation 的映射在 SPI 内：`Write` 生成 `Write` 和 `ApplyPatch`，`Exec` 生成 `Exec`，`Watch` 不生成 runtime operation，见 `crates/agentdash-spi/src/connector/mod.rs:194` 到 `crates/agentdash-spi/src/connector/mod.rs:219`。
- `agentdash-application-vfs` 只是导出 wrapper：`compile_whole_mount_runtime_vfs_access_policy(vfs)` 调用 `RuntimeVfsAccessPolicy::whole_mounts_from_vfs(vfs)`，见 `crates/agentdash-application-vfs/src/access_policy.rs:5` 到 `crates/agentdash-application-vfs/src/access_policy.rs:13`。
- matcher 入口只要求 mount、normalized path、operation 同时匹配：`policy.admits(...)` 在 `crates/agentdash-spi/src/connector/mod.rs:180` 到 `crates/agentdash-spi/src/connector/mod.rs:191`；application wrapper 在 `crates/agentdash-application-vfs/src/access_policy.rs:16` 到 `crates/agentdash-application-vfs/src/access_policy.rs:23`。
- 现有 tests 已覆盖 prefix boundary、mount/operation/path 三者同时匹配、whole-mount compiler 保留 provider capability operation、source 记录，见 `crates/agentdash-application-vfs/src/access_policy.rs:63`、`crates/agentdash-application-vfs/src/access_policy.rs:85`、`crates/agentdash-application-vfs/src/access_policy.rs:124`、`crates/agentdash-application-vfs/src/access_policy.rs:193`。

### Current carrier and enforcement path

- `SharedRuntimeVfs` 的 state 明确同时携带 `Vfs` 和 `RuntimeVfsAccessPolicy`，见 `crates/agentdash-application-vfs/src/tools/common.rs:48` 到 `crates/agentdash-application-vfs/src/tools/common.rs:52`；默认 `new(vfs)` 仍使用 whole-mount compiler，见 `crates/agentdash-application-vfs/src/tools/common.rs:55` 到 `crates/agentdash-application-vfs/src/tools/common.rs:60`。
- runtime tools 从 `ExecutionContext` 构造 `SharedRuntimeVfs` 时优先消费 `context.session.vfs_access_policy`，缺失时 fallback 到 whole-mount compiler，见 `crates/agentdash-application/src/runtime_tools/provider.rs:90` 到 `crates/agentdash-application/src/runtime_tools/provider.rs:101`。
- `VfsService::resolve_provider_dispatch` 是主要共享 enforcement 点：先 `resolve_mount(vfs, mount_id, capability)` 校验 provider support，再 `normalize_mount_relative_path(raw_path, allow_empty)`，再 `ensure_runtime_vfs_access(policy, &mount.id, &path, operation)`，见 `crates/agentdash-application-vfs/src/service.rs:148` 到 `crates/agentdash-application-vfs/src/service.rs:172`。
- `ensure_runtime_vfs_access` 只检查 runtime policy，不再把 mount capability 当授权事实源；deny 文案带 mount/path/operation，见 `crates/agentdash-application-vfs/src/service.rs:31` 到 `crates/agentdash-application-vfs/src/service.rs:49`。
- read/list/suggest/search/exec 等 provider calls 复用 `resolve_provider_dispatch`，例如 read 传 `MountCapability::Read + RuntimeVfsOperation::Read`，见 `crates/agentdash-application-vfs/src/service.rs:321` 到 `crates/agentdash-application-vfs/src/service.rs:335`；exec 传 `MountCapability::Exec + RuntimeVfsOperation::Exec`，见 `crates/agentdash-application-vfs/src/service.rs:1148` 到 `crates/agentdash-application-vfs/src/service.rs:1163`；search 传 `MountCapability::Search + RuntimeVfsOperation::Search`，见 `crates/agentdash-application-vfs/src/service.rs:1223` 到 `crates/agentdash-application-vfs/src/service.rs:1238`。
- multi-mount `apply_patch` 在 parse/normalize 每个 patch target 后对 primary target 和 move target 都检查 `RuntimeVfsOperation::ApplyPatch`，见 `crates/agentdash-application-vfs/src/service.rs:913` 到 `crates/agentdash-application-vfs/src/service.rs:938`。
- `fs_read` tool 先从 `SharedRuntimeVfs` 拿 state，再对 normalized target 做 read policy check，之后调用 `stat_with_policy/read_*_with_policy`，见 `crates/agentdash-application-vfs/src/tools/fs/read.rs:135` 到 `crates/agentdash-application-vfs/src/tools/fs/read.rs:164`。
- `fs_apply_patch` tool 传入 `Some(&access_policy)` 到 `apply_patch_multi_with_policy`，见 `crates/agentdash-application-vfs/src/tools/fs/apply_patch.rs:124` 到 `crates/agentdash-application-vfs/src/tools/fs/apply_patch.rs:139`。
- `shell_exec` 在 cwd target 上显式检查 `RuntimeVfsOperation::Exec`，materialization rewrite 也传入同一 policy，再通过 `exec_with_policy` 执行，见 `crates/agentdash-application-vfs/src/tools/fs/shell.rs:183` 到 `crates/agentdash-application-vfs/src/tools/fs/shell.rs:209` 和 `crates/agentdash-application-vfs/src/tools/fs/shell.rs:270` 到 `crates/agentdash-application-vfs/src/tools/fs/shell.rs:284`。
- platform shell 的 root `ls` 只展示 policy admits List 的 mounts，file operations 都调用 `*_with_policy`，见 `crates/agentdash-application-vfs/src/tools/fs/platform_shell.rs:183` 到 `crates/agentdash-application-vfs/src/tools/fs/platform_shell.rs:205`、`crates/agentdash-application-vfs/src/tools/fs/platform_shell.rs:400` 到 `crates/agentdash-application-vfs/src/tools/fs/platform_shell.rs:435`。
- shell materialization rewrite 先要求 exec mount 支持 `Exec`，再检查 exec cwd 的 `Exec` policy，同时每个 VFS URI source target 都检查 `Read` policy，见 `crates/agentdash-application-vfs/src/materialization.rs:78` 到 `crates/agentdash-application-vfs/src/materialization.rs:106`。
- relay MCP JSON argument/local path materialization 在 `local_path_for_uri` 内 parse URI、resolve read mount、检查 `RuntimeVfsOperation::Read`，见 `crates/agentdash-application-vfs/src/materialization.rs:265` 到 `crates/agentdash-application-vfs/src/materialization.rs:281`。

### Current runtime/session assembly and stale fact sources

- `LaunchPlan::build` 目前从 `launch_envelope.launch_vfs()` 编译 `vfs_access_policy = Some(RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs))`，然后放进 `ExecutionSessionFrame`，见 `crates/agentdash-application-runtime-session/src/session/launch/plan.rs:176` 到 `crates/agentdash-application-runtime-session/src/session/launch/plan.rs:182` 和 `crates/agentdash-application-runtime-session/src/session/launch/plan.rs:288` 到 `crates/agentdash-application-runtime-session/src/session/launch/plan.rs:296`。
- launch test 已断言 session frame 内有 policy 且 admits workspace read，见 `crates/agentdash-application-runtime-session/src/session/launch/plan.rs:570` 到 `crates/agentdash-application-runtime-session/src/session/launch/plan.rs:579`。
- AgentRun runtime surface query 目前从 frame typed VFS whole-mount 编译 policy，并把 policy 放到 `AgentRunRuntimeSurface`，见 `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:334` 到 `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:342` 和 `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:435` 到 `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:438`。
- AgentRun runtime surface update helper 同样 whole-mount 编译 policy，见 `crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs:809` 到 `crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs:819`。
- MCP relay call context 已携带 `context.session.vfs_access_policy.clone()`，所以 typed policy 只要进入 `ExecutionSessionFrame` 就能影响 relay MCP materialization，见 `crates/agentdash-application-runtime-session/src/session/tool_assembly.rs:55` 到 `crates/agentdash-application-runtime-session/src/session/tool_assembly.rs:68`。
- `CapabilityState.vfs.active` 仍在 frame activation 时保存 typed VFS surface，见 `crates/agentdash-application-agentrun/src/agent_run/frame/builder.rs:63` 到 `crates/agentdash-application-agentrun/src/agent_run/frame/builder.rs:66`；builder 也能从 `CapabilityState` 拆出 vfs surface，见 `crates/agentdash-application-agentrun/src/agent_run/frame/builder.rs:136` 到 `crates/agentdash-application-agentrun/src/agent_run/frame/builder.rs:153`。这应继续表达 address space / mount exposure，不应成为 path authorization 事实源。

### Current PermissionGrant facts are not typed VFS rules

- `PermissionGrant` 聚合根的 `requested_paths` 是 `Vec<ToolCapabilityPath>`，字段定义见 `crates/agentdash-domain/src/permission/entity.rs:29` 到 `crates/agentdash-domain/src/permission/entity.rs:30`，构造函数也只接受 `Vec<ToolCapabilityPath>`，见 `crates/agentdash-domain/src/permission/entity.rs:51` 到 `crates/agentdash-domain/src/permission/entity.rs:56`。
- `ToolCapabilityPath` 只有 `capability: String` 和 `tool: Option<String>`，并序列化为 `"cap"` 或 `"cap::tool"`，见 `crates/agentdash-domain/src/workflow/value_objects/capability.rs:30` 到 `crates/agentdash-domain/src/workflow/value_objects/capability.rs:37`、`crates/agentdash-domain/src/workflow/value_objects/capability.rs:93` 到 `crates/agentdash-domain/src/workflow/value_objects/capability.rs:108`。它没有 mount id、normalized path pattern、operation set，也禁止多级 `::`，见 `crates/agentdash-domain/src/workflow/value_objects/capability.rs:135` 到 `crates/agentdash-domain/src/workflow/value_objects/capability.rs:137`。
- `PermissionGrantCompiler` 当前只把每个 requested path 编译为 tool dimension `CapabilityDeclarationRecord`，payload 是 `ToolCapabilityDirective::Add/Remove(path)`，见 `crates/agentdash-application/src/permission/compiler.rs:27` 到 `crates/agentdash-application/src/permission/compiler.rs:47`。
- AgentRun effective grant projection 明确只把 tool-level paths 作为 tool admission：`classify_path` 中 `path.tool.is_some()` 是 admission projection，否则是 AgentFrame surface revision，见 `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:59` 到 `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:64`；`add_admission_paths` 只记录 capability -> tool set，见 `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:87` 到 `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:96`。
- permission runtime surface update 在 approve/revoke 后只对 `surface_paths` 更新 `CapabilityState.tool`，没有任何 VFS policy 更新路径，见 `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs:118` 到 `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs:156` 和 `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs:323` 到 `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs:371`。
- tests 已保护 tool-level grants 不扩大 schema-facing `CapabilityState`，但没有 VFS policy projection，见 `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:635` 到 `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:679` 和 `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:681` 到 `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:718`。

### Project VFS preset exposure is provider/mount cropping, not authorization

- `AgentPresetConfig.project_vfs_mount_exposure_grants` 文档已写明该字段只描述 Project VFS mount exposure，runtime mount/path admission 由 `RuntimeVfsAccessPolicy` 表达，见 `crates/agentdash-domain/src/common/agent_config.rs:68` 到 `crates/agentdash-domain/src/common/agent_config.rs:73`。
- `AgentPresetConfig::from_json` 遇到旧 `vfs_access_grants` 直接报错，见 `crates/agentdash-domain/src/common/agent_config.rs:144` 到 `crates/agentdash-domain/src/common/agent_config.rs:154`，测试在 `crates/agentdash-domain/src/common/agent_config.rs:439` 到 `crates/agentdash-domain/src/common/agent_config.rs:445`。
- `ProjectVfsMountExposureGrant` 只有 `mount_id` 和 `capabilities`，见 `crates/agentdash-domain/src/common/agent_config.rs:273` 到 `crates/agentdash-domain/src/common/agent_config.rs:278`。
- `apply_project_vfs_mount_exposure_grants` 只处理带 Project VFS mount metadata 的 mounts；没有 grant 的 Project VFS mount 清空 capabilities，已有 grant 与 mount 原 capability 取交集，不处理非 Project VFS runtime mounts，见 `crates/agentdash-application-vfs/src/mount_project.rs:136` 到 `crates/agentdash-application-vfs/src/mount_project.rs:167`。
- 对应测试明确 Project VFS mount exposure grants 不约束 agent memory mount，见 `crates/agentdash-application-vfs/src/mount_project.rs:483` 到 `crates/agentdash-application-vfs/src/mount_project.rs:503`。
- ProjectAgent composer/owner bootstrap 只把 preset exposure 传入 frame construction 并作用于 VFS composition，见 `crates/agentdash-application/src/frame_construction/composer_project_agent.rs:105` 到 `crates/agentdash-application/src/frame_construction/composer_project_agent.rs:114`、`crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:364` 到 `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:373`、`crates/agentdash-application/src/frame_construction/request_assembler.rs:298` 到 `crates/agentdash-application/src/frame_construction/request_assembler.rs:309`。

### Where typed PermissionGrant VFS rules should compile

Typed VFS grant facts should be owned and compiled by the AgentRun permission/runtime surface owner, not by the VFS provider layer and not by `CapabilityState.vfs`.

Reasoning:

- `PermissionGrant` lifecycle, approval, revoke, effect frame, and runtime session anchors already live in the permission + AgentRun runtime surface path. The code that decides whether a grant affects admission vs frame surface is `AgentRunGrantProjection` / `PermissionRuntimeSurfaceUpdateService`; this is the natural owner to partition future grant facts by kind.
- VFS providers only know provider support and storage semantics. If provider mount capability is mutated from grants, it conflates support with authorization and recreates the old error this task is cleaning up.
- `CapabilityState.vfs.active` and AgentFrame `vfs_surface_json` should remain typed VFS address-space snapshots. If grants write path authorization into `CapabilityState.vfs`, every consumer of `Vfs` becomes a hidden authorization consumer and `RuntimeVfsAccessPolicy` stops being the single runtime admission carrier.
- The existing connector/runtime tool path already consumes `ExecutionSessionFrame.vfs_access_policy`. Therefore typed grant rules should enter before `LaunchPlan::build` finalizes `ExecutionSessionFrame`, and runtime hot-update/adoption paths should refresh `AgentRunRuntimeSurface.vfs_access_policy` alongside tool surface updates.

Recommended owner split:

1. Domain adds a typed VFS grant fact beside, not inside, `ToolCapabilityPath`, e.g. `PermissionGrantRequest::ToolPaths(Vec<ToolCapabilityPath>)` plus `PermissionGrantRequest::VfsRules(Vec<PermissionGrantVfsRule>)`, or equivalent typed field. The VFS rule must contain mount/surface identity, normalized path pattern, operations, and source/grant id.
2. `agentdash-application/src/permission/compiler.rs` remains responsible for compiling tool capability transitions only. Add a separate named compiler for VFS policy contributions, preferably in `agentdash-application-agentrun` where runtime session/frame anchor context exists, or in `agentdash-application` only if it stays pure and takes typed grant facts plus owner context as input.
3. `agentdash-application-agentrun` should combine base whole-mount/project/system policy with active typed grant rules when building `AgentRunRuntimeSurface.vfs_access_policy` and `ExecutionSessionFrame.vfs_access_policy`. The combination must produce the final policy object, not mutate `Vfs.mounts` or `CapabilityState.vfs.active`.
4. `agentdash-application-vfs` should remain the policy model/matcher/enforcement owner. It can expose a pure function to merge/validate policy rules, but it should not query grants or decide permission lifecycle.

### Minimal implementation steps and file order

1. Define typed grant contract in domain:
   - Add typed VFS grant value object under `crates/agentdash-domain/src/permission/` or adjacent permission value objects.
   - Add it to `PermissionGrant` as a typed field or discriminated request collection.
   - Keep `ToolCapabilityPath` untouched as tool capability syntax; do not overload it with `vfs:mount/path` strings.
2. Split current permission compiler responsibilities:
   - Keep `crates/agentdash-application/src/permission/compiler.rs` limited to tool declarations.
   - Add a VFS policy rule compiler that maps typed VFS grant facts to `RuntimeVfsAccessRule { source: PermissionGrant }`.
   - Validate mount id/path pattern/operation shape before producing runtime rules; path must already be normalized or normalized through the same VFS path utility before matching.
3. Add AgentRun policy assembly as the single owner:
   - In `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs`, extend grant projection or add a sibling projection that loads active grants by frame and partitions typed VFS grant facts separately from tool admission.
   - In `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs`, update approve/revoke flow so VFS grant changes refresh runtime surface policy, but do not update `CapabilityState.vfs.active` or mount capabilities.
4. Replace whole-mount-only runtime surface builders:
   - In `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs`, replace direct `RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs)` with the AgentRun policy assembly result.
   - In `crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs`, update helpers/tests to use assembled policy instead of recomputing whole-mount policy.
   - In `crates/agentdash-application-runtime-session/src/session/launch/plan.rs`, consume the already-assembled launch/runtime surface policy rather than compiling from `launch_vfs()` locally. This avoids creating a second fact path at launch time.
5. Remove/close fallback policy compilation paths after call sites supply policy:
   - `crates/agentdash-application/src/runtime_tools/provider.rs` should require `context.session.vfs_access_policy` when `vfs` exists once all launch paths supply it.
   - `SharedRuntimeVfs::new(vfs)` and service/materialization `None => whole_mounts_from_vfs` fallbacks can be narrowed to tests or removed from production call paths once no caller omits policy.
6. Keep VFS enforcement unchanged except for test coverage:
   - `crates/agentdash-application-vfs/src/service.rs`, `tools/fs/*`, and `materialization.rs` already accept policy and enforce after normalization. They should not gain grant repository access or a new authorization service.

### Old assumptions to delete, rename, or narrow

- Delete the assumption that `RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs)` is a durable authorization compiler. It is only a bootstrap/system projection for current whole-mount behavior.
- Narrow `RuntimeVfsAccessSource::ProjectPreset` usage: ProjectAgent preset exposure is not a path authorization source by itself; it can explain the initial whole-mount exposure rule only after frame construction has already cropped Project VFS mounts.
- Delete any future attempt to encode VFS grant semantics in `ToolCapabilityPath` strings. Current parser only supports tool capability paths and disallows the richer shape needed for mount/path/operation.
- Remove production fallback where missing `vfs_access_policy` silently recompiles whole-mount policy after typed grant rules exist. That fallback would bypass grant revocation/path narrowing.
- Do not write typed grant effects into `CapabilityState.vfs.active`. `CapabilityState.vfs` is an address-space/surface snapshot, while `RuntimeVfsAccessPolicy` is the runtime admission carrier.
- Do not mutate `Mount.capabilities` from permission grants. Mount capabilities are provider support / Project VFS exposure intersection, not per-session authorization.
- Avoid introducing a parallel `VfsAccessService` that reads grants at enforcement time. Enforcement should consume the already-compiled policy object; grant lifecycle belongs upstream.

### Targeted tests

- Domain permission test: `PermissionGrant` can carry a typed VFS rule with mount id, prefix/root path pattern, operation set, and rejects malformed absolute/escape paths. No broad build.
- Permission compiler/projection test: tool `requested_paths` still compile to tool `CapabilityDirective`; typed VFS rules compile to `RuntimeVfsAccessRule { source: PermissionGrant }` and do not alter tool declarations.
- AgentRun runtime surface test: active typed VFS grant for `main://docs` read enters `AgentRunRuntimeSurface.vfs_access_policy`; `CapabilityState.vfs.active` and `Vfs.mounts[*].capabilities` are unchanged.
- Launch plan test: `ExecutionSessionFrame.vfs_access_policy` preserves the assembled typed grant rule and does not recompute whole-mount-only policy from `vfs`.
- Runtime surface update test: approving/revoking a typed VFS grant updates the runtime surface policy and active tools use the new policy snapshot.
- VFS service test: mount supports `Read` but policy only admits prefix `docs`; `read_text_with_policy(main://src/lib.rs)` denies after normalization, while `read_text_with_policy(main://docs/readme.md)` admits.
- apply_patch test: policy admits `ApplyPatch` only under prefix `allowed`; primary path and move target outside prefix deny.
- shell/materialization test: shell cwd requires `Exec` policy and VFS URI materialization requires `Read` policy; mount capability alone is insufficient.
- Negative search/static check: `rg -n "whole_mounts_from_vfs\\(&vfs\\)|compile_whole_mount_runtime_vfs_access_policy\\(&vfs\\)" crates/agentdash-application-runtime-session crates/agentdash-application-agentrun crates/agentdash-application` should not show production launch/runtime surface recomputation paths after integration.
- No broad Rust build recommendation; use narrow Rust test filters matching the new modules, e.g. permission VFS rule tests, AgentRun runtime surface policy tests, VFS access policy/service deny tests, and materialization policy deny tests.

### External references

- None. This research is internal-code/spec only; no external versioned API or third-party docs were needed.

### Related specs

- `.trellis/spec/backend/vfs/vfs-access.md` - primary contract for VFS address model, Project VFS mount exposure, runtime tools, and provider/runtime policy boundary.
- `.trellis/spec/backend/vfs/vfs-materialization.md` - materialization policy contract for shell/MCP URI rewrites.
- `.trellis/spec/backend/session/execution-context-frames.md` - connector-facing `ExecutionContext` projection contract; needs follow-up update to include `vfs_access_policy`.

## Caveats / Not Found

- No typed `PermissionGrant` VFS rule contract currently exists. The only persisted grant request path shape found is `ToolCapabilityPath`.
- No current production code compiles active `PermissionGrant` facts into `RuntimeVfsAccessPolicy`; current policy assembly is whole-mount from `Vfs`.
- `RuntimeVfsAccessSource::PermissionGrant` exists and tests use it, but it is only a policy model source enum today, not wired to real grants.
- Several production call sites still fallback to whole-mount compilation when policy is absent. That is acceptable for the current MVP but becomes a bypass once typed VFS grant rules exist.
- `.trellis/spec/backend/session/execution-context-frames.md` is stale relative to code because it does not document `ExecutionSessionFrame.vfs_access_policy`.
