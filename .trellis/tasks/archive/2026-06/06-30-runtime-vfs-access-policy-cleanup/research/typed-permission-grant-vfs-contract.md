# Research: typed PermissionGrant VFS path-rule contract

- Query: 只读研究 PermissionGrant typed VFS path-rule contract 的最小收束方案
- Scope: internal
- Date: 2026-06-30

## Findings

### Files found

- `.trellis/tasks/06-30-runtime-vfs-access-policy-cleanup/prd.md` - 当前 D9 任务要求 PermissionGrant VFS path rules 投影到 runtime policy，且不能用 tool-level grant 扩大 mount/path 访问。
- `.trellis/tasks/06-30-runtime-vfs-access-policy-cleanup/design.md` - 已确认当前 `PermissionGrant.requested_paths` 只有 `ToolCapabilityPath`，不能从字符串伪造 VFS path policy。
- `.trellis/tasks/06-30-runtime-vfs-access-policy-cleanup/implement.md` - 已落地 `RuntimeVfsAccessPolicy` carrier/enforcement，剩余 gap 是 typed PermissionGrant VFS path rules。
- `.trellis/spec/backend/permission/architecture.md` - Permission System 是 runtime capability grant 的事实源，policy 评估输入当前是 `requested_paths`。
- `.trellis/spec/backend/permission/grant-lifecycle.md` - 记录了现有 `PermissionGrant` / repository / API / schema contract，当前 `requested_paths` 是 `Vec<ToolCapabilityPath>`。
- `.trellis/spec/backend/vfs/vfs-access.md` - VFS 地址模型是 `surface_ref + mount_id + mount_relative_path`，runtime mount/provider capability 与 runtime policy 分工明确。
- `.trellis/spec/backend/database-guidelines.md` - 普通 schema 变更新增 migration，不修改已提交 migration；复杂值对象 JSON 文本列使用业务语义名。
- `.trellis/spec/backend/domain-payload-typing.md` - 高频业务路径必须类型化，不能继续用裸字符串/`Value` 承载核心语义。
- `crates/agentdash-domain/src/permission/entity.rs` - `PermissionGrant` aggregate root，当前 `requested_paths: Vec<ToolCapabilityPath>`。
- `crates/agentdash-domain/src/permission/value_objects.rs` - grant scope/status/policy 和 `ScopeEscalationIntent.unlocked_paths` 当前也使用 `ToolCapabilityPath`。
- `crates/agentdash-domain/src/workflow/value_objects/capability.rs` - `ToolCapabilityPath` 的真实结构、parse 和 serde string 形态。
- `crates/agentdash-application/src/permission/policy.rs` - policy 只按 `ToolCapabilityPath` 自动审批池判断。
- `crates/agentdash-application/src/permission/service.rs` - `GrantRequest` 和 request lifecycle 只接收 `Vec<ToolCapabilityPath>`。
- `crates/agentdash-application/src/permission/compiler.rs` - 独立 compiler 把 grant paths 编为 tool capability directives。
- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs` - active grant projection 用 tool-level path 作为 AgentRun admission。
- `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs` - 另一套 grant-to-tool directive / frame surface update 编译路径。
- `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs` - repository 把 `requested_paths` 作为 JSON value 读写。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` - 当前 `permission_grants.requested_paths jsonb NOT NULL`。
- `crates/agentdash-api/src/routes/permission_grants.rs` - REST response 把 `ToolCapabilityPath` 降成 qualified string。
- `crates/agentdash-contracts/src/system/permission.rs` - API contract 暴露 `requested_paths: Vec<String>`。
- `crates/agentdash-application/src/companion/payload_types.rs` - capability grant request payload 校验 `requested_paths` 是非空字符串数组，并逐个走 `ToolCapabilityPath::parse`。
- `crates/agentdash-application/src/companion/tools.rs` - companion payload JSON schema 声明 `requested_paths.items` 为 string。
- `crates/agentdash-spi/src/connector/mod.rs` - 已有 `RuntimeVfsOperation`、`RuntimeVfsPathPattern`、`RuntimeVfsAccessSource`、`RuntimeVfsAccessPolicy`。
- `crates/agentdash-application-vfs/src/service.rs` - VFS provider dispatch 已在 normalize 后执行 runtime policy admission。
- `crates/agentdash-application-ports/src/vfs_surface_runtime.rs` - 已有 typed `ResolvedVfsSurfaceSource` 与 stable `surface_ref()`/parser。

### Current `requested_paths` / `ToolCapabilityPath` facts

`ToolCapabilityPath` 是 tool capability 地址，不是 VFS 地址。它只有 `capability: String` 和 `tool: Option<String>` 两段，注释明确 JSON 形式是 `"file_read"`、`"file_read::fs_grep"` 这类 qualified string；没有 surface、mount、path、operations 字段。证据：`crates/agentdash-domain/src/workflow/value_objects/capability.rs:23`、`:31`、`:33`、`:34`、`:35`、`:36`。

`ToolCapabilityPath::parse` 只允许无 `::` 的 capability 级 path 或恰好一个 `::` 的 tool 级 path，多级分隔直接拒绝。这意味着类似 `session-runtime:sess/main/src/read` 这类 VFS 语义不能被它可靠表达；强行塞字符串只能形成伪语义。证据：`crates/agentdash-domain/src/workflow/value_objects/capability.rs:101`、`:108`、`:115`、`:116`、`:135`。

Serde 形态是单个字符串：`Serialize` 调 `to_qualified_string()`，`Deserialize` 先读 `String` 再 `parse`。证据：`crates/agentdash-domain/src/workflow/value_objects/capability.rs:148`、`:150`、`:154`、`:156`、`:157`。

`PermissionGrant` 当前持有 `requested_paths: Vec<ToolCapabilityPath>`，构造函数也只接收这个类型。证据：`crates/agentdash-domain/src/permission/entity.rs:17`、`:30`、`:54`、`:69`。

`ScopeEscalationIntent.unlocked_paths` 也仍是 `Vec<ToolCapabilityPath>`，因此它表达的是 capability 解锁，不应被扩展成 VFS path grant。证据：`crates/agentdash-domain/src/permission/value_objects.rs:100`、`:104`。

Repository create 时把 `grant.requested_paths` 直接 `serde_json::to_value` 绑定到 `requested_paths`，读出时从 `serde_json::Value` 反序列化为 `Vec<ToolCapabilityPath>`。证据：`crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:29`、`:32`、`:43`、`:249`、`:272`、`:273`。

当前 schema 是 `requested_paths jsonb NOT NULL`，不是 spec 文档里早期示例的 `TEXT`。证据：`crates/agentdash-infrastructure/migrations/0001_init.sql:369`、`:375`。

REST response 和 generated TS contract 都把 requested paths 暴露为 string array。API mapper 调 `to_qualified_string()`；contract 是 `requested_paths: Vec<String>`；前端生成类型是 `requested_paths: Array<string>`。证据：`crates/agentdash-api/src/routes/permission_grants.rs:101`、`:107`、`:110`、`crates/agentdash-contracts/src/system/permission.rs:72`、`:80`、`packages/app-web/src/generated/permission-contracts.ts:6`。

Companion grant request 入口也只接受字符串数组，并用 `ToolCapabilityPath::parse` 校验。证据：`crates/agentdash-application/src/companion/payload_types.rs:259`、`:260`、`:276`、`crates/agentdash-application/src/companion/tools.rs:1937`、`:1940`、`:1942`。

Application policy 当前只把 `requested_paths` 当 capability paths：自动审批池来自 `ProjectAgent.config.auto_grantable_capabilities` 与 `AgentProcedureContract.requestable_capabilities` 的交集，覆盖规则只比较 `capability` / `tool` / `*`。证据：`crates/agentdash-application/src/permission/policy.rs:20`、`:24`、`:36`、`:44`、`:133`、`:139`、`:143`。

Grant effect 当前也只投影 tool capability。`PermissionGrantCompiler` 对每个 path 生成 `ToolCapabilityDirective::Add/Remove`，source 是 `permission_grant`。证据：`crates/agentdash-application/src/permission/compiler.rs:17`、`:29`、`:34`。AgentRun runtime surface update 中还有一套同类编译逻辑，证据：`crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs:216`、`:220`、`:226`、`:227`。

AgentRun active grant projection 用 `path.tool.is_some()` 判定 admission-only，用 `None` 判定写 AgentFrame surface revision；这仍然是 tool/capability 维度，不是 VFS mount/path 维度。证据：`crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:25`、`:27`、`:51`、`:59`、`:60`、`:67`、`:87`。

### Existing runtime VFS policy carrier

项目已有 runtime VFS policy 值对象：`RuntimeVfsOperation` 包含 read/list/search/write/exec/apply_patch；`RuntimeVfsPathPattern` 有 `All`/`Prefix`；`RuntimeVfsAccessSource` 有 `ProjectPreset`、`PermissionGrant`、`SystemRuntimeProjection`；`RuntimeVfsAccessRule` 有 `mount_id`、`path_pattern`、`operations`、`source`；`RuntimeVfsAccessPolicy.admits` 按 mount、operation、normalized path 匹配。证据：`crates/agentdash-spi/src/connector/mod.rs:98`、`:111`、`:131`、`:139`、`:140`、`:180`。

当前 whole-mount compiler 从 `Vfs` mount capabilities 推导 operation set。`Write` 同时产生 `Write` 和 `ApplyPatch`，`Exec` 产生 `Exec`。证据：`crates/agentdash-spi/src/connector/mod.rs:160`、`:165`、`:169`、`:171`、`:194`、`:209`、`:211`、`:213`。

VFS dispatch 已有正确 enforcement 点：`resolve_provider_dispatch` 先 `resolve_mount`，再 `normalize_mount_relative_path`，再选择/编译 policy 并调用 `ensure_runtime_vfs_access`，最后才取 provider。证据：`crates/agentdash-application-vfs/src/service.rs:148`、`:159`、`:162`、`:164`、`:171`、`:172`。`ensure_runtime_vfs_access` 拒绝时报告 `{mount_id}://{normalized_path}`。证据：`crates/agentdash-application-vfs/src/service.rs:31`、`:37`、`:47`。

VFS surface identity 也已有 typed source：`ResolvedVfsSurfaceSource` 包括 Project/Story/Task preview、SessionRuntime、AgentRun、ProjectSkillAssets、ProjectVfsMount、ProjectAgentKnowledge，并有 `surface_ref()` 和 `parse_surface_ref()`。证据：`crates/agentdash-application-ports/src/vfs_surface_runtime.rs:14`、`:16`、`:28`、`:31`、`:38`、`:42`、`:48`、`:76`。

`Mount` 和 `MountCapability` 已在 domain common，不在 SPI 私有层。证据：`crates/agentdash-domain/src/common/mount.rs:5`、`:7`、`:12`、`crates/agentdash-domain/src/common/mount_capability.rs:3`、`:6`。

### Minimal typed VFS grant contract

最小收束方向不是在 `requested_paths: Vec<String>` 上加解析约定，而是把 PermissionGrant 的申请项改成 typed access item。推荐目标形态：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PermissionGrantRequestedAccess {
    ToolCapability {
        path: ToolCapabilityPath,
    },
    VfsPathRule {
        rule: PermissionGrantVfsPathRule,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionGrantVfsPathRule {
    pub surface: PermissionGrantVfsSurfaceRef,
    pub mount_id: String,
    pub path_scope: RuntimeVfsPathPattern,
    pub operations: BTreeSet<RuntimeVfsOperation>,
    pub source: RuntimeVfsAccessSource,
}
```

`source` 在 stored grant rule 中应固定/校验为 `PermissionGrant`，投影到 `RuntimeVfsAccessRule` 时继续使用 `RuntimeVfsAccessSource::PermissionGrant`。Project preset 和 system/runtime projection 仍由现有 whole-mount compiler / runtime projection 输入产生，不进入 PermissionGrant stored rule。

`surface` 必须是 typed surface identity，不是裸字符串拆分。最小可行做法是把 `ResolvedVfsSurfaceSource` 的 stable enum 上移到可被 domain 使用的位置，或在 domain 定义同等 typed `PermissionGrantVfsSurfaceRef` 并让 application 层只通过现有 parser/formatter 做边界转换。考虑到 `Mount` / `MountCapability` 已经在 domain common，较干净的收束是把 `RuntimeVfsOperation` 和 `RuntimeVfsPathPattern` 也移到 domain common，由 SPI re-export/复用，避免 permission 和 runtime 各维护一套 operation 枚举。

`path_scope` 只保留 `All` / `Prefix(normalized_mount_relative_path)`。不要新增 glob。原因是现有 runtime policy matcher 已按 normalized prefix 边界处理，且当前 PermissionGrant 没有事实要求 glob 语义。证据：`crates/agentdash-spi/src/connector/mod.rs:116`、`:121`、`:124`。

`operations` 应直接使用 runtime operation set：read/list/search/write/exec/apply_patch。不要从 tool name、mount capability 字符串或 `file_write` 推导。mount/provider support 仍在 VFS dispatch 中由 `MountCapability` 校验，grant 只贡献 runtime admission rule；最终生效仍是 tool visibility、mount support、runtime policy 三者交集。

编译策略：

1. PermissionGrant policy 先按 typed access item 分类：tool item 走现有 tool capability policy；VFS item 走 VFS-specific requestability/approval policy。
2. Approved active grants 投影时，tool item 只进入 tool capability directive/admission projection；VFS item 只进入 `RuntimeVfsAccessPolicy` rules，不写 `CapabilityState.vfs` / mount provider capabilities。
3. Runtime/session assembly 针对当前 surface 编译 active grant VFS rules：只有 `rule.surface` 匹配当前 resolved surface 时，才生成 `RuntimeVfsAccessRule { mount_id, path_pattern, operations, source: PermissionGrant }`。
4. 生成的 grant policy rules 与 Project preset / system runtime projection rules 合并为 runtime policy carrier，继续走现有 normalized dispatch enforcement。

### Files that need implementation changes

Domain:

- `crates/agentdash-domain/src/permission/value_objects.rs` - 新增 `PermissionGrantRequestedAccess`、`PermissionGrantVfsPathRule`、typed surface ref；把 `ScopeEscalationIntent.unlocked_paths` 改名/收窄为 tool-only，或改成同一 typed access item。
- `crates/agentdash-domain/src/permission/entity.rs` - 将 `requested_paths: Vec<ToolCapabilityPath>` 替换为 typed `requested_access: Vec<PermissionGrantRequestedAccess>`；构造函数和测试同步。
- `crates/agentdash-domain/src/permission/repository.rs` - trait 名称不一定变，但文档和 tests 应使用 requested access 语义。
- `crates/agentdash-domain/src/workflow/value_objects/capability.rs` - 保留 `ToolCapabilityPath` 为 tool-only；不再作为 PermissionGrant 的唯一 request item。
- `crates/agentdash-domain/src/common/*` - 推荐承接共享 `RuntimeVfsOperation` / `RuntimeVfsPathPattern` / surface identity 值对象，SPI 复用。

Application:

- `crates/agentdash-application/src/permission/service.rs` - `GrantRequest` 改为 typed requested access；request lifecycle 分类 tool/VFS rules；拒绝空 access set。
- `crates/agentdash-application/src/permission/policy.rs` - 旧 `evaluate(requested_paths, ...)` 收窄为 tool policy；新增 VFS rule policy，不能把 VFS rule 混入 `ToolCapabilityPath` 自动批准池。
- `crates/agentdash-application/src/permission/compiler.rs` - 删除或并入唯一 owner。当前它和 AgentRun surface update 重复编译 tool directive；typed VFS 加入后不应有两套 grant effect compiler。
- `crates/agentdash-application/src/permission/escalation.rs` - `unlocked_paths` 如果保留，应明确改成 `unlocked_tool_paths`；如果 escalation 也能解锁 VFS，应改用同一 typed access item。
- `crates/agentdash-application/src/companion/payload_types.rs`、`crates/agentdash-application/src/companion/tools.rs` - capability grant request payload 从 `requested_paths: string[]` 改成 typed `requested_access` array schema。
- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs` - `AgentRunGrantProjection` 只消费 tool access item；不要扫描 VFS rules。
- `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs` - 唯一化 grant-to-runtime effect projection；surface-changing tool items 写 frame revision，VFS rules 编译成 runtime VFS policy contribution。
- `crates/agentdash-application-runtime-session/src/session/launch/plan.rs` / runtime surface assembly owners - 当前使用 `RuntimeVfsAccessPolicy::whole_mounts_from_vfs` 的位置需要合并 active PermissionGrant VFS rules。
- `crates/agentdash-application-vfs/src/access_policy.rs` - 保留 matcher；增加从 typed PermissionGrant VFS rule 到 runtime policy rule 的小型 compiler/helper。

Infrastructure:

- `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs` - `GrantRow` 改读写 `requested_access`；`create` bind typed JSON；`TryFrom` 反序列化 typed enum；错误上下文用 `permission_grants.requested_access`。
- `crates/agentdash-infrastructure/migrations/0034_permission_grant_requested_access.sql` - 新增 migration，增加 `requested_access jsonb`，把旧 `requested_paths` 数据迁为 `[{ "kind": "tool_capability", "path": "<old>" }]`，再 `DROP COLUMN IF EXISTS requested_paths`。普通任务不要改 `0001_init.sql`。

API / contracts:

- `crates/agentdash-contracts/src/system/permission.rs` - `PermissionGrantResponse.requested_paths: Vec<String>` 改成 typed `requested_access: Vec<PermissionGrantRequestedAccessDto>`；新增 VFS rule DTO。
- `crates/agentdash-contracts/src/generate_ts.rs` - 导出新增 DTO。
- `crates/agentdash-api/src/routes/permission_grants.rs` - mapper 不再 `to_qualified_string()`；按 typed item 映射 tool/VFS response。
- `packages/app-web/src/generated/permission-contracts.ts` - 由 contract generator 更新，不手写。
- `packages/app-web/src/features/permission/PermissionGrantCard.tsx` - 展示 typed access item，VFS rule 展示 surface/mount/path/operations/source。

### Old paths to delete or narrow

- 删除 `PermissionGrant.requested_paths` 作为 grant 总入口；替换为 `requested_access`。不要保留 `requested_paths` alias。
- 删除 API response `requested_paths: Vec<String>`；替换为 typed DTO。不要新增并行 `requested_vfs_paths` 同时保留旧 `requested_paths`。
- 删除 companion payload `requested_paths: string[]`；替换为 typed `requested_access`。如果仍需要短期 tool grant 输入，也必须是 `{"kind":"tool_capability","path":"..."}`，不是旧字段。
- 收窄 `ToolCapabilityPath` 的语义和注释：它只表达 tool capability/tool admission，不表达 VFS mount/path。
- 收窄或改名 `ScopeEscalationIntent.unlocked_paths` 为 `unlocked_tool_paths`；否则会继续暗示任意 path。
- 删除/合并 `PermissionGrantCompiler` 与 AgentRun `compile_permission_grant_transition` 的重复编译路径，避免 typed VFS rules 在两处发散。
- 不把 PermissionGrant VFS rules 写入 `CapabilityState.vfs` 或 provider mount `capabilities`；Project preset exposure 和 provider support 仍保持独立事实源。
- 不添加 string convention，如 `vfs:surface:mount:path:ops`、`file_write::main/src`、`mcp/server/path` 等。

### Targeted tests and migration guard

Domain unit tests:

- `PermissionGrantRequestedAccess` serde: tool item JSON roundtrip、VFS item JSON roundtrip。
- VFS rule requires non-empty `mount_id`、non-empty operation set、normalized prefix path；absolute path 和 `..` escape rejected before storage or during constructor.
- `ToolCapabilityPath` tests保留 tool-only string shape；新增测试证明 VFS-looking multi-segment string is rejected or not accepted as VFS rule.

Application tests:

- Policy test: tool grant still follows `agent_auto_grantable ∩ lifecycle_requestable`；VFS rule does not enter that pool.
- Projection test: active grant with tool item only changes tool admission/surface；active grant with VFS item only contributes runtime VFS policy, not AgentFrame tool surface。
- Runtime policy compiler test: grant VFS rule with unmatched `surface` is ignored；matched surface emits `RuntimeVfsAccessRule` with source `PermissionGrant`。
- Intersection test: PermissionGrant VFS rule for read on `main/src` cannot write/apply_patch/exec；mount supports operation but policy denies path still denied；tool-level grant does not expand mount/path access。

VFS targeted tests:

- Existing `agentdash-application-vfs` prefix matcher tests should remain; add a compiler test for typed grant rule to prefix policy.
- Existing service tests around read/write/apply_patch/shell deny should get one case where the deny comes from absent PermissionGrant rule while mount capability exists.

Repository / migration tests:

- Repository roundtrip creates grant with mixed `tool_capability` and `vfs_path_rule` items and reads the same typed structure.
- Bad JSON in `permission_grants.requested_access` returns `DomainError::InvalidConfig` with that column name.
- Migration guard: add only `0034_permission_grant_requested_access.sql`; run `pnpm run migration:guard`.
- Migration data guard: seed a row with old `requested_paths = '["story_management","task::read"]'::jsonb`, run migration, assert `requested_access` becomes typed tool items and `requested_paths` column is gone.

Suggested narrow validation commands for implementation phase:

```powershell
pnpm run migration:guard
cargo test -p agentdash-domain permission --lib
cargo test -p agentdash-application permission --lib
cargo test -p agentdash-application-agentrun permission_runtime_surface --lib
cargo test -p agentdash-application-vfs access_policy --lib
rg -n "requested_paths|unlocked_paths|PermissionGrantCompiler|compile_permission_grant_transition" crates packages --glob "!target" --glob "!node_modules"
```

## External references

- None. This research used internal task docs, specs, and code only.

## Related specs

- `.trellis/spec/backend/permission/architecture.md` - PermissionGrant 是授权事实源，compiler output 必须走 runtime capability pipeline。
- `.trellis/spec/backend/permission/grant-lifecycle.md` - 当前 lifecycle/schema/API contract 需要更新为 typed access item。
- `.trellis/spec/backend/vfs/vfs-access.md` - VFS address model、Project VFS mount exposure、runtime policy 三者分工。
- `.trellis/spec/backend/database-guidelines.md` - 新增 migration、不要修改已提交 migration；列名用业务语义。
- `.trellis/spec/backend/domain-payload-typing.md` - PermissionGrant request 是高频业务路径，必须类型化。

## Caveats / Not Found

- 未发现现有 PermissionGrant typed VFS path-rule contract；当前所有 grant request/effect/API/schema 都围绕 `ToolCapabilityPath` string array。
- 已有 `RuntimeVfsAccessPolicy` 不包含 `surface_ref` 字段，因为它随 resolved runtime `Vfs` surface 携带。PermissionGrant stored VFS rule 必须携带 surface identity，编译时按当前 surface 过滤。
- 当前 `RuntimeVfsOperation` / `RuntimeVfsPathPattern` 在 SPI 层；为了让 domain PermissionGrant 直接持有这些类型，建议上移到 domain common 或引入一个明确的共享 crate。复制一套 domain-only enum 风险较高。
- 本研究未运行 Rust 编译或 broad tests，符合只读研究约束。
