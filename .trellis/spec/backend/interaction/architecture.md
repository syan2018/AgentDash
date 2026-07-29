# Interaction Architecture

## Role

Interaction 子系统承载 Human 与 Agent 共同操作的版本化交互定义和共享运行态。Canvas 是
`InteractionDefinitionRevision(kind = canvas)` 的 authoring / presentation 产品形态；
`InteractionInstance` 是 canonical state、revision、command 和 event 的唯一事实源。

## Stable V1 Identities

| Identity | Meaning |
| --- | --- |
| `canvas:{definition_id}` | Workspace Module 中的 Canvas definition/module identity |
| `canvas://{definition_id}` | Canvas authoring / preview presentation URI |
| `interaction:{instance_id}` | Workspace Module 中的 shared runtime identity |
| `interaction://{instance_id}` | Interaction renderer presentation URI |
| `definition_revision_id` | immutable definition revision identity |
| `source_bundle_digest` | immutable Canvas source content identity |

VFS mount、Attachment、PresentationState 和 RendererLease 是独立生命周期对象，不复用 definition 或
instance identity。`RuntimeSession` 只属于 Agent connector delivery / trace，不参与上述 identity、owner、
authorization 或 retention。

## Definition And Source

```text
InteractionDefinitionRevision {
  definition_id,
  revision_id,
  owner: User(user_id) | Project(project_id),
  kind: canvas,
  definition_format_version: 1,
  interaction_contract_version: 1,
  source_bundle,
  command_bindings,
  action_bindings,
  component_bindings,
  resource_slots,
  lineage,
  created_at
}

SourceBundle {
  format_version: 1,
  entry_file,
  files,
  sandbox_config,
  import_map,
  digest
}
```

- Definition revision immutable；编辑提交 `base_revision_id`，通过 optimistic CAS 生成新 revision。
- Canvas VFS changeset 对同一个 base revision 批量提交文件变更并生成一个新 `SourceBundle`；客户端草稿
  只属于 UI local state，不建立 durable draft、CRDT 或隐式 last-write-wins。
- Personal definition 是 User-owned；Project definition 是 Project-owned。publish 从 exact Personal
  revision 创建或更新独立 Project definition，并记录 source revision lineage；copy 创建独立 User
  definition；unpublish/archive 只移除目录可见性。
- 被 instance、artifact 或 lineage 引用的 revision/source bundle 保持可寻址。Extension promotion 固定
  exact definition revision 与 source bundle digest。
- Definition 只声明 resource slots。`RuntimeBinding` 把 slot 绑定到已经授权的 resource、artifact 或
  provider ref；binding handle 本身不是 capability，也不写回 immutable source。

## Instance Ownership And Lifetime

```text
InteractionInstance {
  instance_id,
  owner: User(user_id) | Project(project_id),
  definition_revision_id,
  interaction_contract_version: 1,
  state,
  state_revision,
  status: open | closed,
  pinned_artifacts,
  retention,
  created_at,
  updated_at
}
```

- Instance 继承 definition scope：Personal definition 产生 User-owned instance；Project definition 产生
  Project-owned instance。
- Instance 固定 exact definition revision 和 Extension artifact digest。Extension upgrade 只影响新
  definition / new instance；artifact 缺失时返回 structured unavailable，不静默 rebind。
- AgentRun 可以通过 Attachment 连接 instance，但 tab、RendererLease、AgentRun 或 RuntimeSession 结束
  不删除 instance。生命周期只由 explicit close 与 retention policy 推进。
- PresentationState 是用户/renderer 局部展示偏好，不进入 canonical state。RendererLease 只表达活跃
  renderer 与 subscription 租约。

## Command Transaction

Human 与 Agent 使用同一个 typed command use case：

```text
InteractionCommandRequest {
  instance_id,
  command_id,
  command_type,
  handler_version,
  payload,
  expected_state_revision,
  actor,
  origin,
  optional_attachment_ref
}
```

Command 对 Agent 的 actor policy 只有 `direct` 与 `human_only`。`human_only` 拒绝 Agent canonical write；
Agent 可以通过 Channel attention 发送非权威建议，Human 必须重新提交正式 command。

V1 通用 mutation handler 为 `state_patch_v1`：

- payload 只允许有界 JSON Patch `add`、`remove`、`replace`；
- path 必须落在 definition 声明的 JSON Pointer allowlist；
- command 校验 payload schema、state schema、patch count、state size 和 expected revision；
- `command_id` 在 instance scope 内幂等；相同 id + 不同 digest 返回 conflict；
- command receipt、ordered event、next state revision 与可选 effect intent 在同一事务提交。

Definition-level action binding 负责 Canvas source 的普通按钮和其它 host action：iframe 只提交
`action_key + payload`，immutable revision 固定 platform command、Operation 或 OperationScript target
及 exact Operation allowlist。它不依赖 Extension artifact。

Component event binding 只属于已安装 Extension UI component，instance 创建时解析并 pin exact artifact；
其事件可以复用相同 action 执行边界。平台不执行 Extension/Canvas reducer code，也不维护 generic
reducer registry、mapping DSL 或 proposal aggregate。

## Reliable Effect Boundary

Canonical command 可以在同一事务写入一个 `OperationEffectIntent`：

```text
OperationEffectIntent {
  effect_id,
  instance_id,
  source_event_id,
  operation_ref,
  validated_input,
  principal_scope_snapshot,
  idempotency_key,
  status,
  attempt,
  next_attempt_at
}
```

只有 descriptor 声明 `replay_safe | idempotent` 的单 Operation 可进入 outbox。Replay 仍重新执行 current
capability/readiness admission，并以稳定 effect/idempotency identity 收敛到单一成功结果。即时
OperationScript 不自动 replay；多步、可恢复、带 gate 的副作用进入 Workflow。

## Projection And Communication

- Query/subscription 从 instance state revision 与 ordered event cursor 生成；前端 refresh 后重新读取
  canonical projection，不用 iframe observation 或 local store 推断共享状态。
- Agent projection 可裁剪 state/event，但必须携带 instance identity、definition revision 与 state revision。
- Channel 只接收 typed attention ref/summary；Interaction command/event/state body 不复制到 Channel、Mailbox、
  LifecycleGate 或 notification。
- Workspace Module 只暴露 definition/instance discovery、typed command、attachment 与 presentation projection，
  不拥有第二套 state、Operation catalog 或 Extension dispatch。

## Version And Migration Contract

- `definition_format_version`、`interaction_contract_version`、platform handler identity、Component ABI、
  OperationRef 和 OperationScript dialect/host API 从 V1 起显式版本化。
- Breaking behavior 通过新 V2 reader/handler/ABI 与显式 per-version migration 引入；既有 revision/instance
  继续按 pin 的 V1 语义运行。
- 项目首次落地直接建立最终 Interaction schema，并移除旧 Canvas aggregate/runtime snapshot/state schema、
  route、DTO、repository 与 frontend consumer。当前没有生产存量，因此 fixtures/seed/examples 直接按
  V1 重建。

## Required Validation

- CAS conflict、command idempotency、stale revision、actor policy 与 owner authorization。
- `state_patch_v1` allowlist、schema、size/patch limits 与 event/state atomicity。
- OperationEffectIntent 原子写入、claim/replay、稳定 idempotency 与 admission failure。
- Definition publish/copy/unpublish lineage、SourceBundle digest、VFS changeset 与 resource binding。
- exact definition/artifact pin、artifact unavailable、explicit close/retention 与 renderer reload。
- repository scan 确认旧 Canvas state authority 与 Session-bound Interaction authority 已清除。

## Scenario: Exact Interaction Runtime And Promotion

### 1. Scope / Trigger

当 authoring SourceBundle 被固定为 Definition revision、创建运行 Instance、提交 command、订阅呈现状态，
或把 revision 生成 Extension 时，使用本合同。

### 2. Signatures

```text
create_revision(definition_id, source_bundle) -> immutable revision
create_instance(definition_id, revision_id) -> instance
submit_command(instance_id, command_id, payload, expected_state_revision) -> receipt
list_events(instance_id, after_sequence) -> ordered events
put_presentation(instance_id, expected_revision, state) -> presentation
acquire_or_release_renderer_lease(instance_id, expected_revision, owner) -> lease
promote_extension(definition_id, revision_id) -> standard Extension package
```

### 3. Contracts

- Instance 固定 exact revision；command、schema、Component 与 promotion 均从该 revision 解析。
- command receipt、event、state 与 effect intent 在同一事务提交。
- presentation 与 renderer lease 是独立 CAS projection，不修改 canonical Interaction state。
- Agent command 必须同时满足 descriptor visibility、current AgentFrame attachment 与
  `can_submit_commands`。
- dynamic provider 使用 `interaction/{definition_id}.{revision_id}` identity，并复用
  `workspace_module_invoke` admission。
- promotion 读取 immutable SourceBundle，生成标准 webview Extension package 和 exact provenance。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| revision/definition 不匹配 | `not_found` / `conflict` |
| expected state revision 过期 | 返回 revision conflict |
| command_id digest 冲突 | `conflict` |
| Agent 缺少 attachment/capability | `forbidden` |
| presentation/lease revision 过期 | 独立 CAS conflict |
| SourceBundle digest 或 entry 无效 | promotion 拒绝 |

### 5. Good / Base / Bad Cases

- Good：Agent 通过 exact Operation 提交 command，UI 用 cursor 读取 event 并刷新 canonical state。
- Base：Human 提交普通 command 并持有单一 renderer lease。
- Bad：从 iframe local state 反推 canonical state，或从 mutable Canvas workspace 直接打包。

### 6. Tests Required

- immutable revision、instance pin、command CAS/idempotency/actor policy。
- event sequence、presentation state、renderer lease。
- AgentFrame attachment 与 WorkspaceModule/Operation provider projection。
- SourceBundle digest、exact revision promotion 与 Extension provenance。
- PostgreSQL transaction、migration 与生成合同检查。

### 7. Wrong vs Correct

```text
Boundary mismatch:
mutable authoring state -> runtime/session snapshot -> packaged artifact

Canonical:
SourceBundle revision -> Interaction Instance / standard Extension package
```

## Scenario: Definition Revision Persistence Contract

### 1. Scope / Trigger

当 `InteractionDefinitionRevision` 新增或修改 contract 字段，或 repository 调整 revision 的 PostgreSQL
编码时，使用本合同。Repository port 保留，因为 immutable revision 是跨重启业务事实；数据库 adapter
只负责把规范化的 revision identity、SourceBundle、lineage 与 contract 重组为 domain revision。

### 2. Signatures

```sql
interaction_definition_revisions (
  revision_id uuid,
  definition_id uuid,
  revision_number bigint,
  project_id text,
  owner_kind text,
  owner_id text,
  source_bundle_digest text,
  contract jsonb,
  created_at timestamptz
)
```

```rust
struct PersistedDefinitionRevisionContract {
    kind: InteractionDefinitionKind,
    definition_format_version: u16,
    interaction_contract_version: u16,
    authoring_mount_id: String,
    title: String,
    description: String,
    initial_state: Value,
    state_schema: Value,
    agent_projection: InteractionAgentProjection,
    command_definitions: Vec<InteractionCommandDefinition>,
    component_bindings: Vec<ComponentBinding>,
    action_bindings: Vec<InteractionActionBinding>,
    resource_slots: Vec<ResourceSlotDefinition>,
    created_by: String,
}
```

### 3. Contracts

- `revision_id`、definition/owner/revision identity、`source_bundle_digest` 与 `created_at` 只存 scalar column，
  不复制进 `contract`。
- `contract` 只保存 revision 的版本化 Interaction 行为与 authoring metadata。
- SourceBundle files 和 lineage 继续使用各自规范化表；repository 读取后重组并调用
  `InteractionDefinitionRevision::validate()`。
- contract 字段变化必须新增顺序 migration，在 migration 中一次性形成完整目标 JSON shape；生产 reader
  只读取目标 shape，不维护多版本 document fallback。
- Interaction persistence/serialization 进入 HTTP 500 前必须通过 `diag_error!` 记录完整 source/debug；
  HTTP trace span 以 info 级携带 method/URI，且不记录 headers。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| contract 缺少 required field 或字段类型错误 | `InteractionError::Serialization`，API 记录诊断并返回固定 500 |
| scalar UUID / revision number 非法 | 带精确 `table.column` 的 serialization/invalid field |
| SourceBundle digest 无对应内容或校验失败 | `not_found` / persistence conflict |
| owner scalar 与 Project/domain invariant 不一致 | domain `validate()` 拒绝 |
| contract 新增数组字段 | migration 物化完整数组并增加 shape check |

### 5. Good / Base / Bad Cases

- Good：新增 `action_bindings` 时，顺序 migration 为每条 revision 写入数组，并在同一 migration 删除被替代
  的 document shape。
- Base：新 revision 写 scalar identity + typed contract，读取时与 SourceBundle/lineage 重组。
- Bad：在 repository 私有结构中再复制一份完整 domain revision identity，或依赖 `serde(default)` 把未迁移的
  durable schema 漂移静默吞掉。

### 6. Tests Required

- contract serialization 断言不包含 scalar identity/digest/timestamp。
- migration 断言新增字段被物化、JSON type check 生效、被替代列删除。
- repository create/get roundtrip 覆盖空与非空 action/component/command/resource bindings。
- API error mapping 断言 persistence/serialization 返回固定 500，并由 diagnostics 记录 operation/stage。
- `pnpm run migration:guard`、真实 PostgreSQL migration runner 与受影响 Rust tests。

### 7. Wrong vs Correct

```text
Boundary mismatch:
scalar identity + full revision document(identity + digest + contract) + normalized source/lineage

Canonical:
scalar identity/digest + typed contract + normalized SourceBundle/lineage -> validated domain revision
```
