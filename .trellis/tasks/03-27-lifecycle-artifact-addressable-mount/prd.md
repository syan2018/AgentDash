# Lifecycle 产出收集与寻址空间 Mount 重构

## 背景

当前 lifecycle 系统的骨架已经就位：领域模型（LifecycleDefinition → LifecycleRun → StepState → RecordArtifact）、状态机（Ready → Running → Completed）、hook 集成（snapshot 投影 → completion 判定 → 自动推进）、前端展示（step 列表 + artifact 渲染）均已可用。

但它目前主要服务于**流程编排**（确保 agent 按正确顺序执行 step），而不是**知识沉淀**（记录执行路径、保存上下文、支持复盘和跨 session 记忆）。本任务旨在补全从"编排引擎"到"可复盘、可寻址的知识容器"的缺口。

## Goal

拓展 workflow/lifecycle 系统，使其：
1. 通过 hook 在关键执行节点自动记录 agent 的执行路径与上下文
2. 将结构化产出存入 lifecycle run，使其能被复盘
3. 将 lifecycle 数据暴露为 agent 可使用的寻址空间 mount，使短期记忆可跨上下文被追溯

## 现状分析

### 已有能力
- `LifecycleRun.record_artifacts: Vec<WorkflowRecordArtifact>` — step 完成时可携带产物
- `WorkflowArtifactReportTool` — agent 可在执行期间主动上报 artifact
- `build_step_completion_artifact_drafts` — 自动从 completion decision 生成 PhaseNote
- Hook snapshot 中 `metadata.active_workflow` 包含完整的 lifecycle/step/contract 投影
- 前端 `TaskWorkflowPanel` 可渲染 artifact 列表

### 关键缺口

**缺口 1：Hook 不记录"执行路径"本身**
- `HookTraceEntry` / `HookDiagnosticEntry` 是瞬态观测数据，不写入 `LifecycleRun`
- `record_artifacts` 只有 step 完成时的"总结性产物"，没有过程性执行轨迹
- `WorkflowRecordArtifactType` 只有 5 种（SessionSummary / JournalUpdate / ArchiveSuggestion / PhaseNote / ChecklistEvidence），缺少面向复盘的类型

**缺口 2：Lifecycle 没有"上下文快照"能力**
- `EffectiveSessionContract` 在每次 `load_session_snapshot` 时动态解析，但不持久化到 `LifecycleRun`
- Step 完成后无法回溯"当时生效的 contract、注入的指令、绑定的上下文"
- `LifecycleStepState` 只有 `summary: Option<String>`，缺少结构化上下文字段

**缺口 3：Lifecycle 不是 agent 可寻址的空间**
- `AddressSpaceProvider` 只有 3 个内置 provider（workspace_file / workspace_snapshot / mcp_resource）
- Lifecycle 数据不在任何 mount 上，agent 无法通过统一接口访问
- 唯一访问路径是 `WorkflowArtifactReportTool`（只写不读）和 hook snapshot 里的当前 step 投影

---

## 重构计划

### Phase 1：丰富 lifecycle 的产出收集模型

**目标**：让 lifecycle 从"只有完成总结"扩展到"过程轨迹 + 上下文快照 + 决策记录"。

#### 1.1 扩展 `WorkflowRecordArtifactType` 枚举

在 `crates/agentdash-domain/src/workflow/value_objects.rs` 中新增：

```rust
pub enum WorkflowRecordArtifactType {
    // 现有
    SessionSummary,
    JournalUpdate,
    ArchiveSuggestion,
    PhaseNote,
    ChecklistEvidence,
    // 新增
    ExecutionTrace,       // hook 执行轨迹摘要
    DecisionRecord,       // 关键决策记录（约束拦截、completion 判定等）
    ContextSnapshot,      // step 级上下文快照（生效 contract + metadata）
    HookDiagnosticSummary, // hook diagnostic 聚合
}
```

**影响范围**：
- `value_objects.rs` — 枚举定义
- `completion.rs` — `workflow_artifact_type_tag` 函数
- `execution_hooks.rs` — `workflow_record_artifact_type_tag` / `parse_workflow_record_artifact_type_tag`
- `dto/workflow.rs` — DTO 映射
- `workflow_repository.rs` — JSON 序列化兼容（serde 自动处理）
- 前端 `types/index.ts` — TypeScript 类型
- 前端 `shared-labels.ts` — 展示文案

#### 1.2 在 `LifecycleStepState` 上增加结构化字段

```rust
pub struct LifecycleStepState {
    pub step_key: String,
    pub status: LifecycleStepExecutionStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
    // 新增
    pub context_snapshot: Option<serde_json::Value>,  // step 完成时的上下文快照
}
```

`context_snapshot` 在 step complete 时由 hook 自动填充，包含：
- 当时生效的 `EffectiveSessionContract`
- 当时的 task_status
- 关键 metadata 片段
- hook diagnostic 摘要

#### 1.3 新增 `LifecycleRun` 上的 `execution_log` 字段

```rust
pub struct LifecycleRun {
    // ...现有字段...
    pub execution_log: Vec<LifecycleExecutionEntry>,  // 新增：有序执行日志
}
```

```rust
pub struct LifecycleExecutionEntry {
    pub timestamp: DateTime<Utc>,
    pub step_key: String,
    pub event_kind: LifecycleExecutionEventKind,
    pub summary: String,
    pub detail: Option<serde_json::Value>,
}

pub enum LifecycleExecutionEventKind {
    StepActivated,
    StepCompleted,
    ConstraintBlocked,     // 约束拦截了 agent 的操作
    CompletionEvaluated,   // completion check 被评估
    ArtifactAppended,      // 产物被追加
    HookIntercepted,       // hook 拦截了 stop/transition
    ContextInjected,       // 上下文被注入到 session
}
```

这是一个**追加式日志**，在 hook 的关键节点自动写入，为复盘提供时间线视图。

---

### Phase 2：Hook 层自动写入执行路径

**目标**：在 `execution_hooks.rs` 的关键节点自动向 `LifecycleRun` 写入执行日志和上下文快照。

#### 2.1 写入时机与内容

| Hook 节点 | 写入内容 | event_kind |
|-----------|---------|------------|
| `load_session_snapshot` 解析到 active workflow | 记录"上下文已注入" | `ContextInjected` |
| `BeforeStop` 被约束拦截 | 记录拦截原因、当时的 signals | `ConstraintBlocked` |
| `BeforeStop` / `SessionTerminal` completion 评估 | 记录 decision 结果 | `CompletionEvaluated` |
| `apply_completion_decision` 设置 `pending_advance` | 记录推进请求 | `CompletionEvaluated` |
| `advance_workflow_step` 成功 | 记录 step 完成 + context_snapshot | `StepCompleted` |
| `WorkflowArtifactReportTool` 被调用 | 记录产物追加 | `ArtifactAppended` |

#### 2.2 实现方式

在 `AppExecutionHookProvider` 上新增方法：

```rust
async fn append_execution_log_entry(
    &self,
    run_id: Uuid,
    entry: LifecycleExecutionEntry,
) -> Result<(), HookError>
```

该方法通过 `workflow_run_repo.get_by_id` + 追加 + `update` 实现。为避免高频写入的性能问题，可考虑：
- 批量写入：在 `evaluate_hook` 结束时一次性 flush
- 或在 `HookResolution` 上增加 `pending_execution_log: Vec<LifecycleExecutionEntry>`，由 `HookSessionRuntime::evaluate` 统一写入

#### 2.3 Step 完成时自动生成 `ContextSnapshot` artifact

在 `advance_workflow_step` 成功后，从当前 snapshot 的 `metadata.active_workflow` 提取关键信息，生成一条 `ContextSnapshot` 类型的 artifact 并追加到 run。

---

### Phase 3：Lifecycle 作为寻址空间 Mount

**目标**：让 agent 能通过统一的 mount + path 接口读取 lifecycle 数据。

#### 3.1 新增 `LifecycleVfsProvider`

在 `crates/agentdash-injection/src/address_space.rs` 中新增：

```rust
pub struct LifecycleVfsProvider;

impl AddressSpaceProvider for LifecycleVfsProvider {
    fn descriptor(&self, _ctx: &AddressSpaceContext<'_>) -> Option<AddressSpaceDescriptor> {
        Some(AddressSpaceDescriptor {
            id: "lifecycle".to_string(),
            label: "Lifecycle 执行记录".to_string(),
            kind: ContextSourceKind::EntityRef, // 或新增 ContextSourceKind::Lifecycle
            provider: "lifecycle_vfs".to_string(),
            supports: vec!["read".to_string(), "list".to_string(), "search".to_string()],
            selector: None,
        })
    }
}
```

注册到 `builtin_address_space_registry`。

#### 3.2 设计虚拟路径语义

```
lifecycle://active/                          → 当前活跃 run 的概览（JSON）
lifecycle://active/steps/                    → 所有 step states 列表
lifecycle://active/steps/{key}/              → 单个 step 的详情 + context_snapshot
lifecycle://active/artifacts/                → 所有 record_artifacts 列表
lifecycle://active/artifacts/{id}            → 单个 artifact 内容
lifecycle://active/log/                      → execution_log 时间线
lifecycle://active/log/?since={timestamp}    → 过滤后的日志
lifecycle://runs/                            → 当前 target 的所有 runs 列表
lifecycle://runs/{run_id}/                   → 指定 run 的概览
```

#### 3.3 在 `build_derived_address_space` 中自动挂载

当 session 绑定了 lifecycle run 时，自动在 `RuntimeAddressSpace.mounts` 中添加 `lifecycle_vfs` mount：

```rust
RuntimeMount {
    id: "lifecycle".to_string(),
    provider: "lifecycle_vfs".to_string(),
    backend_id: String::new(),
    root_ref: format!("lifecycle://target/{target_kind}/{target_id}"),
    capabilities: vec![MountCapability::Read, MountCapability::List, MountCapability::Search],
    default_write: false,
    display_name: Some("Lifecycle 记录".to_string()),
    metadata: serde_json::json!({
        "run_id": active_run.id,
        "lifecycle_key": lifecycle.key,
    }),
}
```

#### 3.4 在 `RelayAddressSpaceService` 中新增 `lifecycle_vfs` 分支

对 `read_text` / `list` / `search_text` 方法，当 `mount.provider == "lifecycle_vfs"` 时，不走 relay，而是：
- 从 `LifecycleRunRepository` 读取数据
- 按虚拟路径语义解析请求
- 序列化为文本/JSON 返回

---

### Phase 4：前端复盘视图增强

**目标**：在前端提供 lifecycle 执行记录的复盘能力。

#### 4.1 Execution Log 时间线视图

在 `TaskWorkflowPanel` 中新增折叠区域，展示 `execution_log` 的时间线：
- 按时间排序的事件列表
- 每个事件显示 event_kind badge + summary + 可展开的 detail
- 支持按 step_key 过滤

#### 4.2 Context Snapshot 对比视图

在 step 详情中，展示该 step 完成时的 `context_snapshot`：
- 生效的 instructions
- 绑定的 context_bindings
- 当时的 task_status
- completion checks 的评估结果

#### 4.3 Artifact 分类展示

按 `artifact_type` 分组展示 artifacts：
- ExecutionTrace / DecisionRecord 归入"执行轨迹"分组
- ContextSnapshot 归入"上下文快照"分组
- PhaseNote / SessionSummary 归入"阶段总结"分组
- ChecklistEvidence 归入"检查证据"分组

---

## 实施顺序与依赖

```
Phase 1（领域模型扩展）
  ├── 1.1 扩展 ArtifactType 枚举
  ├── 1.2 扩展 LifecycleStepState
  └── 1.3 新增 execution_log
        │
Phase 2（Hook 写入管道）  ← 依赖 Phase 1
  ├── 2.1 定义写入时机
  ├── 2.2 实现写入方法
  └── 2.3 自动生成 ContextSnapshot
        │
Phase 3（寻址空间 Mount）  ← 依赖 Phase 1
  ├── 3.1 LifecycleVfsProvider
  ├── 3.2 虚拟路径语义
  ├── 3.3 自动挂载
  └── 3.4 RelayAddressSpaceService 分支
        │
Phase 4（前端增强）  ← 依赖 Phase 1 + 2
  ├── 4.1 时间线视图
  ├── 4.2 上下文快照对比
  └── 4.3 Artifact 分类展示
```

Phase 1 是基础，Phase 2 和 Phase 3 可并行推进，Phase 4 在 Phase 1+2 完成后进行。

## Acceptance Criteria

- [ ] `WorkflowRecordArtifactType` 新增至少 `ExecutionTrace`、`DecisionRecord`、`ContextSnapshot` 三种类型
- [ ] `LifecycleStepState` 支持 `context_snapshot` 字段
- [ ] `LifecycleRun` 支持 `execution_log` 追加式日志
- [ ] Hook 在 `BeforeStop` 拦截、`SessionTerminal` 推进、`advance_workflow_step` 时自动写入 execution_log
- [ ] Step 完成时自动生成 ContextSnapshot artifact
- [ ] `AddressSpaceRegistry` 包含 `lifecycle_vfs` provider
- [ ] Agent 可通过 `lifecycle://active/` 路径读取当前 run 的状态、artifacts、execution_log
- [ ] 前端 `TaskWorkflowPanel` 可展示 execution_log 时间线
- [ ] 前端按 artifact_type 分组展示 artifacts
- [ ] 所有新增字段的 SQLite 持久化正常工作（JSON 列）
- [ ] 现有 lifecycle 功能无回归（step 推进、completion 判定、artifact 上报）

## Technical Notes

- `execution_log` 和 `context_snapshot` 均以 JSON 列存储在 SQLite 的 `lifecycle_runs` 表中，与现有 `step_states` / `record_artifacts` 保持一致的序列化策略
- 高频写入场景（如每次 hook evaluate 都写 execution_log）需要考虑性能：建议在 `HookResolution` 上批量收集，由 `HookSessionRuntime::evaluate` 统一 flush
- `lifecycle_vfs` 的 `read` 操作是从 DB 读取后序列化，不涉及文件系统 I/O
- 虚拟路径中的 `active` 是语义快捷方式，实际解析为 `select_active_run` 的结果
- 前端新增的视图组件应遵循现有 `TaskWorkflowPanel` 的设计语言（rounded-[12px] border、badge 样式等）
