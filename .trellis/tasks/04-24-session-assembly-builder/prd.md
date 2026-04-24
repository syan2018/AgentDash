# Session 组合式装配工厂 + Companion Workflow 支持

> 状态：已完成 Phase 1-3 | 优先级：P1

## 背景

### 当前问题

`SessionRequestAssembler` 通过 4 条硬编码的 compose 函数来组装不同类型的 session：

| 路径 | 函数 |
|---|---|
| ACP Story/Project/Routine | `compose_owner_bootstrap` |
| Task runtime | `compose_task_runtime` |
| Workflow AgentNode | `compose_lifecycle_node` |
| Companion | `compose_companion` |

每条路径都各自处理 VFS、能力、MCP、系统上下文、Prompt、Workflow 等关注点。当需要新的组合（如 "companion + workflow"）时，必须再写一条新的 compose 函数并在其中复制粘贴已有逻辑。

### 目标场景

主 agent 为 companion subagent 分配一份 workflow：
- companion 的 `CompanionSessionContext` 负责父子关系和结果回传
- workflow 的 `LifecycleRun` / `SessionBinding` 提供 `ActiveWorkflowProjection`、port 门禁、capability directives
- 两者的 VFS/MCP/系统上下文注入各有特点，需要能独立叠加

## 已实现：SessionAssemblyBuilder

### 设计

声明式 builder，将 session 装配拆为正交关注点，每个关注点通过独立的 `with_*` 方法注入：

```rust
SessionAssemblyBuilder::new()
    // VFS 层
    .with_vfs(vfs)                                // 直接设置
    .with_companion_vfs(parent_vfs, mode)         // 从父 session 切片
    .append_lifecycle_mount(run_id, key, ports)   // 追加 lifecycle mount
    .append_canvas_mounts(repo, project_id, ids)  // 追加 canvas mount

    // 能力层
    .with_resolved_capabilities(flow, keys)       // 已解析的能力
    .with_companion_capabilities(mode)            // companion 专属裁剪

    // MCP 层
    .with_mcp_servers(servers)                    // 覆盖设置
    .append_mcp_servers(servers)                  // 追加
    .append_relay_mcp_names(names)                // 追加 relay 名

    // 系统上下文 + Prompt + 元信息
    .with_system_context(ctx)
    .with_prompt_blocks(blocks)
    .with_executor_config(config)
    .with_bootstrap_action(action)
    .with_workspace_defaults(workspace)

    // 复合便利方法
    .apply_companion_slice(...)                   // 一步完成 companion 装配
    .apply_lifecycle_activation(...)              // 一步完成 lifecycle node 装配

    .build() → PreparedSessionInputs
```

### 关键原则

1. **每个层独立**：`with_*` 方法只写入自己关注的字段
2. **追加友好**：MCP / relay 等集合字段支持多次 `append`
3. **复合便利**：`apply_companion_slice` / `apply_lifecycle_activation` 封装常见组合
4. **新组合无需新函数**：companion + workflow 只需叠加对应层

## 已实现：Companion + Workflow 组合

### 数据结构

```rust
pub struct CompanionWorkflowSpec<'a> {
    pub companion: CompanionSpec<'a>,
    pub run: &'a LifecycleRun,
    pub lifecycle: &'a LifecycleDefinition,
    pub step: &'a LifecycleStepDefinition,
    pub workflow: Option<&'a WorkflowDefinition>,
}
```

### 流程

```
主 agent companion_request(payload.workflow_key = "code_review")
    │
    ├─ 查找 workflow definition
    ├─ 搜索包含该 workflow 的 lifecycle（entry step 优先，fallback 到任意 step）
    ├─ 创建 LifecycleRun（绑定到 companion session）
    ├─ 创建 SessionBinding(label = "lifecycle_node:{step_key}")
    │
    ▼
compose_companion_with_workflow:
    SessionAssemblyBuilder::new()
        .with_vfs(companion_slice + lifecycle_mount)
        .with_resolved_capabilities(workflow_activation)
        .with_mcp_servers(companion_mcp + workflow_mcp)
        .with_system_context(parent_ctx + workflow_injection)
        .with_prompt_blocks(companion_dispatch_prompt)
        .with_executor_config(...)
        .build()
    │
    ▼
companion session 同时拥有：
  - CompanionSessionContext → 结果回传父级
  - ActiveWorkflowProjection → port 门禁 + capability directives
```

### Tool API

`companion_request` payload 新增可选 `workflow_key` 字段：

```json
{
  "type": "task",
  "prompt": "请 review 当前实现",
  "label": "reviewer",
  "agent_key": "code-reviewer",
  "workflow_key": "code_review"
}
```

## 实施记录

### Phase 1: Builder 骨架 ✅
- `SessionAssemblyBuilder` 结构体（16 个 `with_*` 方法 + 2 个复合方法）
- `build()` → `PreparedSessionInputs`

### Phase 2: 迁移现有 compose ✅
- `compose_companion` → `SessionAssemblyBuilder::apply_companion_slice`
- `compose_lifecycle_node` → `SessionAssemblyBuilder::apply_lifecycle_activation`
- `compose_owner_bootstrap` → builder 组装最终输出

### Phase 3: Companion Workflow 支持 ✅
- `compose_companion_with_workflow()` — builder 组合 companion + workflow 两个层
- `CompanionRequestTool.setup_companion_workflow()` — 查询 workflow/lifecycle、创建 LifecycleRun、创建 binding
- `CompanionRequestTool.find_lifecycle_for_workflow()` — 搜索项目中引用指定 workflow 的 lifecycle
- `build_lifecycle_node_label` 从 `session_association` re-export

### Phase 4: 后续（尚未实施）
- `compose_task_runtime` 迁移到 builder（复杂度最高，返回 `TaskRuntimeOutput`）
- 集成测试
- 前端 companion panel 支持 workflow_key 参数

## 变更文件

| 文件 | 变更 |
|---|---|
| `crates/agentdash-application/src/session/assembler.rs` | 新增 `SessionAssemblyBuilder`；新增 `CompanionWorkflowSpec` / `CompanionWorkflowOutput` / `compose_companion_with_workflow`；迁移 3 个 compose 函数 |
| `crates/agentdash-application/src/session/mod.rs` | 导出新类型 |
| `crates/agentdash-application/src/companion/tools.rs` | `CompanionRequestTool` 新增 `repos` / `platform_config` 字段；新增 `setup_companion_workflow` / `find_lifecycle_for_workflow` 方法；payload 支持 `workflow_key` |
| `crates/agentdash-application/src/vfs/tools/provider.rs` | 更新 `CompanionRequestTool::new` 调用 |
| `crates/agentdash-application/src/workflow/mod.rs` | 新增 `build_lifecycle_node_label` re-export |
