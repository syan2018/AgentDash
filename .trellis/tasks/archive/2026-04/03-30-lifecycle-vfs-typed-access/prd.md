# Lifecycle VFS 按类型访问 + 统一 Locator Resolver

## 背景

当前 `lifecycle_vfs` mount 只支持按 UUID 路径读取 artifact（`active/artifacts/{uuid}`），无法按 node key 或 artifact type 过滤。同时，`WorkflowContextBinding.locator` 虽然在 JSON 定义中声明了，但运行时从未被解析（`resolved_binding_count` 硬编码为 0）。

经讨论明确：**locator 本质上就是 address space URI，所有解析逻辑应收束到统一的 VFS read 路径**。不同 scheme 对应不同 mount + provider，resolver 只做 `parse_mount_uri → read_text`，不按 kind 分叉。

这是 Lifecycle DAG 编排（`04-13-lifecycle-dag-orchestration`）的前置任务 — DAG node 间的上下文传递依赖 `lifecycle://nodes/{key}/artifacts/by-type/{type}` 路径（PRD 决策 D3）。

## 核心原则

**locator = address space URI，解析全部收束到 VFS read。**

```
locator 的 scheme 部分 = mount_id
mount_id → address_space.mounts 中找 mount → mount.provider → MountProviderRegistry 找实现

lifecycle://nodes/research/...     → mount_id="lifecycle"  → provider="lifecycle_vfs"
main://.trellis/workflow.md        → mount_id="main"       → provider="relay_fs"
context://execution_context        → mount_id="context"    → provider="context_vfs"（新增）
```

每个 scheme 背后是一个 mount + provider 对，实现各自的虚拟路径体系。resolver 不关心具体实现 — 只做 URI 解析 + read_text 调用。

## 交付内容

### D1: lifecycle_vfs 路径扩展

在 `provider_lifecycle.rs` 的 `read_text` / `list` 中增加 `nodes/` 路径族：

| 路径 | read_text 返回 | list 返回 |
|------|----------------|-----------|
| `nodes/{key}/state` | LifecycleStepState JSON | — |
| `nodes/{key}/artifacts` | 该 node 所有 artifact JSON 列表 | 每个 artifact 一项 |
| `nodes/{key}/artifacts/by-type/{type}` | 该 type 最新 artifact 内容 | — |
| `nodes/{key}/artifacts/by-type/{type}/list` | 该 type 所有 artifact JSON 列表 | — |
| `nodes/{key}/artifacts/{uuid}` | 指定 artifact 内容（带 step_key 校验） | — |
| `nodes` | 所有 node key 列表 | 每个 node 一项 |

`type` 对应 `WorkflowRecordArtifactType` 的 snake_case 值：`session_summary` / `journal_update` / `phase_note` / `checklist_evidence` / `execution_trace` / `decision_record` / `context_snapshot` / `archive_suggestion`。

现有 `active/steps/{key}` 和 `active/artifacts/{uuid}` 路径保持不变（向后兼容）。

**实现位置**：`crates/agentdash-application/src/address_space/provider_lifecycle.rs`

### D2: context_vfs Provider（新增）

新增 `ContextMountProvider`（provider_id = `"context_vfs"`），处理 `context://` scheme 的运行时上下文读取。

路径体系：

| 路径 | 来源 | 说明 |
|------|------|------|
| `execution_context` | SessionSnapshotMetadata | 当前会话执行上下文 |
| `review_checklist` | WorkflowContract checklist 定义 | Review 检查清单 |
| `story_context_snapshot` | Story.context | Story 上下文快照 |
| `story_prd` | Story.context.prd | Story 需求文档 |
| `workspace_journal` | Workspace 配置 | 工作空间 journal 路径 |
| `project_session_context` | Project 配置 | 项目级会话上下文 |

这些数据来源各不相同（session snapshot、story entity、workspace 配置等），统一在一个 provider 内按路径路由。

**实现位置**：
- 新建 `crates/agentdash-application/src/address_space/provider_context.rs`
- 在 `MountProviderRegistryBuilder::with_builtins` 中注册
- 在 `mount.rs` 中增加 `PROVIDER_CONTEXT_VFS` 常量

### D3: 统一 Locator Resolver

在 session bootstrap / hook injection 阶段，增加通用的 context_bindings 解析能力：

```rust
/// 遍历 contract.injection.context_bindings，
/// 对每个 locator 调用 parse_mount_uri → read_text，
/// 将结果注入为 HookInjection 片段。
async fn resolve_context_bindings(
    bindings: &[WorkflowContextBinding],
    address_space: &AddressSpace,
    service: &RelayAddressSpaceService,
) -> ResolveBindingsOutput {
    for binding in bindings {
        let resource = parse_mount_uri(&binding.locator, address_space)?;
        let content = service.read_text(address_space, &resource, ...).await;
        // 成功 → 注入 HookInjection
        // 失败 + required → 报错
        // 失败 + !required → 记录 warning 跳过
    }
}
```

**调用点**：`workflow_contribution.rs` 的 `build_workflow_step_fragments` 扩展，或 `session_runtime_inputs.rs` 中 session 创建时。

**关键**：resolver 实现不包含任何 provider 特定逻辑 — 只做 URI 解析 + VFS read。

### D4: Builtin JSON locator 迁移

更新 `crates/agentdash-application/src/workflow/builtins/` 下的 JSON 定义：

| 变更 | 说明 |
|------|------|
| `locator` 加 scheme | `execution_context` → `context://execution_context` 等 |
| 移除 `kind` 字段 | JSON 中的 `kind` 从未被 Rust struct 读取，可移除 |
| 文件路径 locator 保持无 scheme | `.trellis/workflow.md` 走 default mount fallback（`main`），无需改动 |

涉及文件：`trellis_dev_task.json`、`trellis_dev_story.json`、`trellis_dev_project.json`

### D5: resolved_binding_count 接通

`WorkflowProjectionSnapshot.resolved_binding_count` 不再硬编码为 0，反映实际解析成功数。

**实现位置**：`crates/agentdash-application/src/workflow/projection.rs`

## 现有基础设施（无需改动）

| 组件 | 已有能力 |
|------|---------|
| `parse_mount_uri` | 按 `://` 拆 mount_id + path，无 scheme 走 default mount |
| `MountProviderRegistry` | HashMap 注册 + dispatch |
| `RelayAddressSpaceService` | 统一 dispatch read_text/list/search_text |
| `build_lifecycle_mount` | 为 session 创建 lifecycle mount |
| `session_runtime_inputs.rs` | session 创建时自动挂载 lifecycle mount |
| `WorkflowRecordArtifact` | 已有 `step_key` + `artifact_type` 字段 |

## 已决策

### L1: locator = address space URI，解析收束到 VFS read

不在 resolver 中按 `kind` 分叉。所有 locator 走统一链路：`parse_mount_uri → find mount → provider.read_text`。

### L2: `nodes/` 作为 lifecycle_vfs 顶层路径段

不在 `active/` 下面。`active/` 暗示"当前活跃 run 的概览"，`nodes/{key}` 暗示"run 内特定 node 的数据"。两者共存，`active/steps/{key}` 保持向后兼容。

### L3: runtime context 走 context_vfs provider

`execution_context`、`review_checklist` 等运行时数据通过新增 `context_vfs` provider 暴露，locator 格式 `context://execution_context`。与 lifecycle_vfs 架构完全一致 — 只是数据来源不同。

### L4: `kind` 字段废弃

`WorkflowContextBinding` 的 Rust struct 从未有 `kind` 字段（JSON 反序列化时被静默丢弃）。locator 的 scheme 部分已隐式承担路由职责，无需显式 `kind`。JSON 中的 `kind` 在迁移时移除。

## 验收标准

- [ ] `lifecycle://nodes/{key}/artifacts/by-type/{type}` 能正确返回指定 node 的指定类型最新 artifact
- [ ] `context://execution_context` 能正确返回当前会话执行上下文
- [ ] Builtin JSON 中所有 `context_bindings` 的 locator 在 session 创建时被解析并注入
- [ ] `resolved_binding_count` 在 snapshot 中反映实际成功数
- [ ] 现有 `active/artifacts/{uuid}` 等路径行为不变
- [ ] 编译通过，现有测试通过

## 建议实现顺序

1. **D1** — lifecycle_vfs 路径扩展（纯增量，独立可测试）
2. **D2** — context_vfs provider 骨架（先支持 execution_context 一条路径验证链路）
3. **D3** — 统一 locator resolver（接通 D1 + D2 的 read 能力）
4. **D4** — Builtin JSON 迁移（locator 加 scheme，移除 kind）
5. **D5** — resolved_binding_count 接通

## 关联任务

- `04-13-lifecycle-dag-orchestration` — DAG 编排依赖 D1 的 `nodes/` 路径 + D3 的 locator resolver（PRD 决策 D3）
- `03-30-session-context-mount` — SessionContextSnapshot Mount 化，与 D2 context_vfs 有交叉

## 状态

**Planning → Ready for Implementation**
