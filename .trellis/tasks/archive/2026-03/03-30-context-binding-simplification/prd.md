# WorkflowContextBinding 简化：去除 BindingKind，统一 Mount 寻址

## 背景

`WorkflowContextBinding` 定义了 workflow step 注入 agent session 的上下文资源声明。
当前设计存在三个问题：

1. **`WorkflowContextBindingKind` 是死代码**
   - 6 种枚举变体（`document_path` / `runtime_context` / `checklist` / `journal_target` / `action_ref` / `artifact_ref`）
   - 后端没有任何 `match binding.kind` 分支逻辑
   - Hook 引擎 (`workflow_contribution.rs`) 完全忽略 `context_bindings` 数组，只读取 `instructions`
   - 唯一消费者是前端 binding-editor 的下拉分类展示和 `build_binding_metadata()` 元数据 API

2. **多数 locator 是空壳声明**
   - 6 类预置 locator 中，只有 `*_session_context` 和 `document_path` 类有实际 resolve 链路
   - `task_review_checklist` / `story_review_checklist` / `workspace_journal` / `workflow_archive_action` / `latest_checklist_evidence` — 仅出现在 `build_binding_metadata()` 中，无任何消费代码

3. **寻址模型未对齐 mount 体系**
   - 项目已有完整的 AddressSpace → Mount → Provider 三层寻址体系
   - `WorkflowContextBinding.locator` 使用自定义字符串标识（如 `"project_session_context"`），与 mount 路径体系割裂
   - 同一份数据可能需要通过两套路径访问（mount path vs. binding locator）

## 目标

1. **删除 `WorkflowContextBindingKind` 枚举和所有空壳 locator 声明**
2. **将 context binding 的寻址统一到 mount 路径体系**（`{mount_id}/{path}` 或 URI）
3. **为规范化 mount 增加目录声明元数据**，让 agent 能理解 mount 内容的索引方式
4. **梳理剩余有价值信息到 mount 的映射方案**（讨论计划，不在本轮实现）

## 设计

### P1：删除 BindingKind 和空壳声明

**删除清单：**

| 删除项 | 位置 | 说明 |
|--------|------|------|
| `WorkflowContextBindingKind` 枚举 | `domain/workflow/value_objects.rs` | 6 个变体，运行时无消费 |
| `WorkflowContextBinding.kind` 字段 | 同上 | 从 struct 中移除 |
| `WorkflowContextBindingKind` TS 类型 | `frontend/types/index.ts` | 前端对应类型 |
| `WorkflowContextBinding.kind` TS 字段 | 同上 | 前端对应字段 |
| `build_binding_metadata()` 函数 | `api/routes/workflows.rs` | ~130 行硬编码元数据，全部是空壳 |
| `GET /workflows/binding-metadata` API | 同上 | 对应的路由 |
| `BindingKindMetadata` / `BindingLocatorOption` 结构体 | 同上 + DTO + 前端类型 | 配套类型 |
| `BINDING_KIND_LABEL` map | `frontend/features/workflow/shared-labels.ts` | 前端 kind 标签 |
| binding-editor 的 kind 下拉 | `frontend/features/workflow/binding-editor.tsx` | UI 改为纯 locator 输入 |

**简化后的 `WorkflowContextBinding`：**

```rust
pub struct WorkflowContextBinding {
    pub locator: String,     // mount 路径或 URI
    pub reason: String,      // 为什么需要此上下文
    pub required: bool,      // 是否必需
    pub title: Option<String>, // 可选显示名
}
```

**DB 兼容**：旧数据中的 `kind` 字段通过 `#[serde(default)]` 静默忽略（`WorkflowContextBinding` 作为 JSON 存在 WorkflowContract 中，不涉及 schema migration）。

### P2：统一 locator 到 mount 路径

当前有价值的 locator 全部可以映射到已有 mount 体系：

| 旧 locator | 对应 mount 路径 | 状态 |
|------------|----------------|------|
| `.trellis/workflow.md` | `main/.trellis/workflow.md` | **已可用** — workspace mount (relay_fs) |
| 任意工作空间相对路径 | `main/{path}` | **已可用** — 同上 |
| `project_session_context` | — | **已可用但未 mount 化** — 通过 `build_*_session_context()` 构建，作为 `SessionContextSnapshot` JSON 返回，目前走 bootstrap 通道而非 mount |
| `story_session_context` | — | 同上 |
| `task_session_context` | — | 同上 |
| `latest_checklist_evidence` | `lifecycle/active/artifacts/{id}` | **间接可用** — lifecycle_vfs 可遍历，但缺少按类型过滤 |
| `task_review_checklist` | — | **不存在** — 空壳 |
| `story_review_checklist` | — | **不存在** — 空壳 |
| `workspace_journal` | — | **不存在** — 空壳 |
| `workflow_archive_action` | — | **不存在** — 空壳 |

**本轮做法**：
- 不存在的 locator 直接删除声明
- 已可用的 workspace 文件路径统一使用 `{mount_id}/{path}` 格式
- `*_session_context` 和 `latest_checklist_evidence` 的 mount 映射留到 P4 讨论

### P3：Mount 目录声明元数据

**动机**：当 mount 挂载后，agent 看到的是扁平的文件/目录列表，缺少"这个 mount 里的内容应该怎么用"的语义提示。对于规范化 mount（如 lifecycle_vfs），目录结构是固定的，应该通过 metadata 告知 agent 索引方式。

**方案**：在 `Mount.metadata` 中增加可选的 `directory_hint` 字段：

```jsonc
// lifecycle mount metadata — 当前
{
  "run_id": "uuid",
  "lifecycle_key": "trellis_dev_task"
}

// lifecycle mount metadata — 增加 directory_hint
{
  "run_id": "uuid",
  "lifecycle_key": "trellis_dev_task",
  "directory_hint": {
    "description": "Lifecycle 执行记录，包含当前 run 的步骤状态和产物",
    "index": [
      { "path": "active", "description": "当前活跃 run 的概览（JSON）" },
      { "path": "active/steps", "description": "各步骤执行状态，子路径为 step_key" },
      { "path": "active/steps/{step_key}", "description": "单步骤详情（JSON）" },
      { "path": "active/artifacts", "description": "产物列表，子路径为 artifact UUID" },
      { "path": "active/artifacts/{id}", "description": "产物内容（纯文本）" },
      { "path": "active/log", "description": "执行日志（JSON 数组）" },
      { "path": "runs", "description": "历史 run 列表" }
    ]
  }
}
```

**实现要点**：
- `directory_hint` 是纯展示性元数据，不影响 provider 的读写逻辑
- 由各 `build_*_mount()` 函数在构建 mount 时注入
- 前端可选择性展示（如 mount inspector 面板），agent 工具调用时可读取用于决策
- inline_fs mount 和 relay_fs mount 也可以携带 `directory_hint`（虽然优先级较低）

### P4：后续任务（已拆分）

| 子项 | 决策 | 追踪 |
|------|------|------|
| 4a. SessionContextSnapshot mount 化 | 独立追踪 | → `03-30-session-context-mount` |
| 4b. Checklist / Journal 概念 | **不保留** — checklist 和 journal_target 均为空壳，lifecycle artifacts + workflow instructions 已覆盖其场景。`JournalUpdate` 作为 artifact type 保留（有消费者），仅删除 `JournalTarget` binding 和 `workspace_journal` locator 声明 |
| 4c. Lifecycle VFS 按类型访问语法 | 独立追踪 | → `03-30-lifecycle-vfs-typed-access` |

## 验收标准

### 本轮实现（P1 + P2 + P3）

- [ ] `WorkflowContextBindingKind` 枚举从 Rust domain 和 TS 类型中删除
- [ ] `WorkflowContextBinding` 不再有 `kind` 字段
- [ ] `build_binding_metadata()` 和 `GET /workflows/binding-metadata` API 删除
- [ ] 前端 binding-editor 不再有 kind 下拉，改为 locator 输入
- [ ] 前端 `BindingKindMetadata` / `BindingLocatorOption` 类型删除
- [ ] 旧数据反序列化不报错（serde 兼容）
- [ ] `build_lifecycle_mount()` 的 metadata 中包含 `directory_hint`
- [ ] `cargo check` 全 crate 通过
- [ ] 前端 typecheck 通过

## 影响范围

| 层 | 文件 | 改动 |
|----|------|------|
| Domain | `workflow/value_objects.rs` | 删除 enum + 字段 |
| API DTO | `dto/workflow.rs` | 删除 kind 映射 |
| API Route | `routes/workflows.rs` | 删除 metadata API + 函数 |
| API Route | `routes/workflows.rs` | 删除路由注册 |
| Application | `address_space/mount.rs` | `build_lifecycle_mount()` 增加 directory_hint |
| Frontend Types | `types/index.ts` | 删除 BindingKind 相关类型 |
| Frontend UI | `binding-editor.tsx` | 简化为纯 locator 输入 |
| Frontend UI | `shared-labels.ts` | 删除 kind label |
| Tests | 各处 | 迁移 fixture 中的 kind 字段 |

## 不动的部分

| 组件 | 原因 |
|------|------|
| `WorkflowContextBinding` struct 本身 | 保留，只删 kind 字段 |
| `WorkflowInjectionSpec.context_bindings` 数组 | 保留，binding 声明机制本身有效 |
| `WorkflowContract` 整体结构 | 不动 |
| Mount / AddressSpace / Provider 三层体系 | 只增加 metadata，不改结构 |
| Hook 引擎核心 | 不动（它本来就不消费 context_bindings） |
