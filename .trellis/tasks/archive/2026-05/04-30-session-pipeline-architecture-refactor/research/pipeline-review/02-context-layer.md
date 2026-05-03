# Research: AgentDashboard 上下文组装层（context / assembler）现状盘点

- **Query**: 摸清 session → agent 管线里上下文组装层的全部路径、冗余、耦合
- **Scope**: internal（Rust 源码）
- **Date**: 2026-04-30
- **Task PRD**: `.trellis/tasks/04-29-cloud-agent-context-bundle-convergence/prd.md`
- **本文目标**：除 PRD 已列出的四个未闭环点之外，盘点还有哪些潜在冗余/耦合，为后续 PR 规划提供数据面依据。

---

## 0. TL;DR（读完本文建议关注的 8 个信号）

1. **`contribute_core_context`（builtins.rs）** 与 **`contribute_story_context`（story/context_builder.rs）** / **`contribute_project_context`（project/context_builder.rs）** 产出的 `ContextFragment` slot/order 高度重叠且互相**不互斥**，任务路径重复渲染 `story` slot 两次。参见 §2.1。
2. `contribute_story_context` / `contribute_project_context` 内部**直接调用** `build_session_plan_fragments`（=把 SessionPlan 烘焙进 Contribution 里），而 `compose_story_step` **又一次独立调用** `build_session_plan_fragments` 并 push 成独立 Contribution，导致 task 路径 / owner 路径的 SessionPlan 产出路径不统一。参见 §2.2。
3. `compose_companion_with_workflow`（assembler.rs:1610+）手动**字符串拼接**一个 `workflow_context` fragment（label=`companion_workflow_injection`），与 `contribute_workflow_binding` / `contribute_lifecycle_context` 的 workflow 片段是**第三处 workflow 渲染实现**。参见 §6.3。
4. `compose_lifecycle_node` 在 `assembler.rs:1263` **已经**调了 `build_session_context_bundle` + `contribute_lifecycle_context`，PRD 第 2 个"未闭环点"描述滞后。真正缺失的是把 `StepActivation.kickoff_prompt` 中 `input_section`/`output_section` 的**结构化产物**保留下来，而不是被揉成一条长文本塞进 `runtime_policy` slot。参见 §1.3。
5. `HookRuntimeDelegate` 已有 `emit_hook_injection_fragments` 调用，挂在 3 个 hook trigger 上（`UserPromptSubmit` / `AfterTurn` / `BeforeStop`）。但它**只 emit audit，不 merge 到 bundle**；`hook_injection_to_fragment` 在 `fragment_bridge.rs` 中的 `From<&SessionHookSnapshot>` 实现**目前没有任何运行时调用点**（只有测试）。参见 §4。
6. Slot 白名单 `RUNTIME_AGENT_CONTEXT_SLOTS` 目前只有**一份**在 `agentdash-spi::context_injection`，但 `HOOK_USER_MESSAGE_SKIP_SLOTS`、`HOOK_SLOT_ORDERS`、`contribute_*` 里硬编码的 order 数字**仍散落**；PiAgent / Vibe Kanban 并没有再维护一份独立白名单（旧 PRD 措辞与现实不符）。参见 §7.1。
7. `mount_file_discovery` 与 `vfs_discovery` 完全不是一回事（前者扫文件读内容，后者对外广播"可发现资源空间"描述符），但模块相邻且命名相近，**容易被误当作重复**。参见 §6.1。
8. `source_resolver` vs `workspace_sources` 确实存在"同一件事两份实现"的倾向：`ContextSourceKind::File` / `ProjectSnapshot` 被 `contribute_declared_sources` 过滤掉后走 `workspace_sources.rs::resolve_workspace_declared_sources`；其余 kind 走 `source_resolver.rs::resolve_declared_sources`。这两条路径的 fragment 构造代码（`fragment_slot` / `fragment_label` / `render_source_section`）几乎**逐行复制**。参见 §6.2。

---

## 1. Contribution → Bundle 的全部来源

`build_session_context_bundle`（`crates/agentdash-application/src/context/builder.rs:103`）是全局**唯一** 合并 reducer。目前生产路径下有 **4 个生产调用点**（不含 `bundle_tests`）：

### 1.1 `SessionRequestAssembler::compose_owner_bootstrap`（assembler.rs:826）

- **Phase**: `ContextBuildPhase::ProjectAgent` or `StoryOwner`（由 `owner_scope_phase` 映射，`assembler.rs:661`）。
- **Contributions 来源**（单个）：
  - `build_owner_context_contribution(owner, vfs, mcp_servers, effective_agent_type, workspace_source_fragments, workspace_source_warnings)`（`assembler.rs:619`），内部只调用一个 domain contributor：
    - `OwnerScope::Story` → `contribute_story_context(StoryContextBuildInput { .. })`（`story/context_builder.rs:29`）
    - `OwnerScope::Project` → `contribute_project_context(ProjectContextBuildInput { .. })`（`project/context_builder.rs:19`）
- **Workspace 声明式来源** 在 Story 分支由 `resolve_workspace_declared_sources` 先行解析（`assembler.rs:797-820`），以 `Vec<ContextFragment>` 形式喂给 `contribute_story_context` 的 `workspace_source_fragments` 字段。Project 分支直接传空数组（Project owner 路径不接 workspace declared sources，见 `assembler.rs:811`）。
- **输出位置**：经 `SessionAssemblyBuilder::with_optional_context_bundle(effective_bundle)`（`assembler.rs:892`）进入 `PreparedSessionInputs.context_bundle`，最终：
  - 由 `finalize_request`（`assembler.rs:105`）写入 `PromptSessionRequest.context_bundle`。
  - `prompt_pipeline.rs:303` 把它作为 `SystemPromptInput.context_bundle` 传给 `assemble_system_prompt`。
- **Audit**：`self.audit_bundle(&context_bundle, audit_session_key, AuditTrigger::SessionBootstrap)`（`assembler.rs:834`）。

**注意**：owner bootstrap **不**产出 owner context resource block 塞进 user prompt blocks —— `prompt_blocks` 直接透传 `spec.user_prompt_blocks`（`assembler.rs:846-870`）。所以"task bootstrap 把 bundle 双重注入"这条 PRD 未闭环点**只存在于 task 路径**，owner 路径已经干净。

### 1.2 `SessionRequestAssembler::compose_story_step`（assembler.rs:1134）

- **Phase**: `ContextBuildPhase::TaskStart` / `TaskContinue`（由 `spec.phase` 映射，`assembler.rs:1050`）。
- **Contributions 来源**（7-9 个 Contribution 按顺序 push）：

| 顺序 | 来源 | 调用位置 | 产出 slot（典型） |
|---|---|---|---|
| 1 | `contribute_core_context(task, story, project, workspace)` | `builtins.rs:159` | `task`(10), `story`(20), `project`(40), `workspace`(50) |
| 2 | `contribute_binding_initial_context(task)` | `builtins.rs:245` | `initial_context`(80) —— 仅当 `agent_binding.initial_context` 非空 |
| 3 | `contribute_declared_sources(task, story)` | `builtins.rs:262` | `requirements`/`constraints`/`codebase`/`references`/`instruction_append`（按 `ContextSlot` 映射）+ 可能的 `references`(89) warnings 条 |
| 4 | `contribute_workspace_static_sources(resolved_workspace_sources.fragments)` | `workspace_sources.rs:19` | 同上 slot 体系（File/Snapshot 类型，base_order=86） |
| 5 | `Contribution::fragments_only([build_declared_source_warning_fragment(...)])` | `builder.rs:159` → `assembler.rs:1069-1077` | `references`(96) —— 仅当 workspace 解析有 warnings |
| 6* | 每个 platform_mcp_config 调 `contribute_mcp(config)` | `builtins.rs:377` | `mcp_config`(85)，同时 `Contribution.mcp_servers` 返回 `RuntimeMcpServer` |
| 7 | `contribute_workflow_binding(workflow, resolved_bindings)` | `workflow_bindings.rs:10` | `workflow_context`(83)、`workflow_context`(84..)、`workflow_context`(89)（warnings） |
| 8 | `contribute_instruction(task, story, workspace, phase, override, additional)` | `builtins.rs:328` | `instruction`(90) —— Override 策略；可能再来一条 `instruction`(100) |
| 9 | `build_session_plan_fragments(SessionPlanInput { .. })`（通过 `Contribution::fragments_only(session_plan.fragments)` 包装） | `plan.rs:93` → `assembler.rs:1099-1132` | `vfs`(35), `tools`(36), `persona`(37), `required_context`(38..), `workflow`(48), `runtime_policy`(49) |

- **输出位置**：
  - `builder.with_context_bundle(context_bundle)`（`assembler.rs:1178`）→ `PreparedSessionInputs.context_bundle`。
  - 关键点：`prompt_blocks` 在 `assembler.rs:1153` 被替换成 `build_story_step_trigger_prompt_blocks(phase)`（固定文本 "请开始执行当前任务。" / "请继续推进当前任务。"）——这是 **2026-04-30 已合入的防重复注入 guard**，由 `assembler.rs:1759 story_step_trigger_prompt_does_not_embed_owner_context` 测试固化。
  - 但真正的用户输入并未完全清洗：见 §1.2b。
- **Audit**：`self.audit_bundle(&context_bundle, ..., AuditTrigger::ComposerRebuild)`（`assembler.rs:1145`）。

#### 1.2b task owner bootstrap 的 **双重** 注入路径

`compose_story_step` 产出的 `prepared.context_bundle` 进到 `build_task_owner_prompt_request`（`routes/acp_sessions.rs:1356`）后，会被 **再读一次** 并渲染成 markdown：

```
routes/acp_sessions.rs:1448
  let task_context_markdown = prepared.context_bundle.as_ref().map(|bundle| {
      bundle.render_section(
          agentdash_spi::FragmentScope::RuntimeAgent,
          agentdash_spi::RUNTIME_AGENT_CONTEXT_SLOTS,
      )
  });
```

这段 markdown 仅在 `SessionPromptLifecycle::RepositoryRehydrate(SystemContext)` 分支被拼进 continuation bundle（见 `routes/acp_sessions.rs:1481`）。`OwnerBootstrap` 分支只保留 `prompt_blocks = user_prompt_blocks`（`routes/acp_sessions.rs:1454` + `1460`）并写 `bootstrap_action = SessionBootstrapAction::OwnerContext`。

- **现状评估**：`OwnerBootstrap` 分支已经**不**把 task context prepend 到 user prompt blocks 里；PRD 描述的 "prompt resource block prepend" 行为在代码里找不到了——这条未闭环点**很可能已经由前置任务关掉**，但 PRD 还没同步。需要跨文交叉验证（本人已把 spec 与实际代码逐字比对，仅余 continuation 路径的 markdown 回灌）。
- **残留**：continuation 路径仍会把 `task_context_markdown` 拼进 continuation；真正的双重注入只剩这一处。且 continuation 路径走 `build_continuation_bundle_from_markdown`（`context/builder.rs:138`）把 markdown 再包一层 `static_fragment` slot，和原 task bundle 的 `task`/`story`/`project` 分离 slot **合并后等于字符串重复**。参见 §1.2c。

#### 1.2c continuation bundle 的重复打包

`build_continuation_bundle_from_markdown`（`context/builder.rs:138`）：
- 产出**单 fragment** bundle，slot=`static_fragment`、source=`session:continuation`。
- 调用点：
  - `routes/acp_sessions.rs:1343`（`resolve_continuation_system_context` 辅助函数，owner 冷启动路径）
  - `routes/acp_sessions.rs:1481-1486`（task 冷启动路径——先把 task bundle 渲染成 markdown，再把 markdown + 事件历史拼一个新 continuation bundle 替换原 bundle）
  - `routine/executor.rs:459`（Routine 冷启动路径）
  - `session/hub.rs:1142`（测试辅助 `owner_bootstrap_request`）

**冗余信号**：task 路径的 continuation 会让 bundle 内出现 `static_fragment` slot。注意 `RUNTIME_AGENT_CONTEXT_SLOTS` 白名单（`context_injection.rs:11`）确实包含 `static_fragment`，所以它最终会进到 PiAgent system prompt 的 `## Project Context` section，但同一段 task 上下文实际上在**同一个 session 的不同 phase** 分别以 "分离 slot" 和 "static_fragment 合并块" 两种形状出现过，给审计轨迹带来异常分片。

### 1.3 `compose_lifecycle_node_with_audit`（assembler.rs:1263）

- **Phase**: `ContextBuildPhase::LifecycleNode`。
- **Contribution 来源**（单个）：`contribute_lifecycle_context(&spec, &activation, &ready_port_keys)`（`assembler.rs:1296`，**定义在 `assembler.rs` 本地**，不在 `context/` 模块）。
  - fragment 1: slot=`workflow_context`, label=`lifecycle_node_context`, order=80，内容是 lifecycle/run/step/node_type/workflow/description/ready_ports 的一段 markdown（`assembler.rs:1331-1339`）。
  - fragment 2（条件）: slot=`workflow_context`, label=`lifecycle_workflow_injection`, order=83，内容是 workflow goal/instructions/context_bindings 渲染（`assembler.rs:1395-1403`）。
  - fragment 3: slot=`runtime_policy`, label=`lifecycle_runtime_policy`, order=84，内容把 `activation.kickoff_prompt.title_line` + `output_section` + `input_section` + capability_keys **揉成一大段文本**（`assembler.rs:1407-1436`）。
- **输出位置**：`SessionAssemblyBuilder::apply_lifecycle_activation(&activation, ...)` 先把 `activation.lifecycle_vfs` / `flow_capabilities` / `mcp_servers` 等写入 builder（`assembler.rs:389`），再 `.with_context_bundle(context_bundle)`（`assembler.rs:1291`）。最终产出 `PreparedSessionInputs.context_bundle`。
- **Audit**：如果传入 `audit_session_key`，会 `emit_bundle_fragments(.., AuditTrigger::ComposerRebuild)`（`assembler.rs:1275-1282`）。
- **PRD 描述滞后**：PRD "未闭环点 2" 说 `compose_lifecycle_node` 不产出 Bundle，但源码已经产出。真正尚欠的是：
  1. fragment 3 把 kickoff prompt 的**三段结构**拍扁成一个 `runtime_policy` slot，导致下游消费者（例如 hook 或审计前端）无法分辨 "complete_lifecycle_node 提醒" / "必须交付的产出" / "输入上下文"。
  2. 没有产出独立的 `lifecycle_activation` trigger —— PRD 里提到过要新增但暂缓。

### 1.4 `compose_companion_with_workflow`（assembler.rs:1565）

**严格来说** 它并不调用 `build_session_context_bundle`，而是**直接** `upsert_by_slot` 到父 bundle 的 clone 上（`assembler.rs:1609-1651`）：

```
let mut merged_bundle = comp.parent_context_bundle.cloned();
if let Some(workflow) = spec.workflow {
    // 手工 String 拼接 goal/instructions
    let workflow_fragment = ContextFragment { slot: "workflow_context", ... };
    match merged_bundle.as_mut() {
        Some(bundle) => bundle.upsert_by_slot(workflow_fragment),
        None => { let mut bundle = SessionContextBundle::new(...); bundle.upsert_by_slot(...); ... }
    }
}
```

这意味着 **第 4 个 Bundle 产出点是"手工 upsert"而不是 reducer**，而且它**跳过了审计总线**（没有任何 `emit_bundle_fragments` 调用）。Companion 路径 Inspector 看不到这条 fragment。参见 §6.3。

### 1.5 Companion 基础分支（非 workflow）

`compose_companion`（`assembler.rs:1445`）直接透传 `parent_context_bundle`（通过 `apply_companion_slice`，`assembler.rs:352`）。
- **没有任何 scope 过滤或 slice_mode 过滤** —— `companion_slice_mode=ConstraintsOnly` 或 `WorkflowOnly` 时，父 bundle 仍然完整继承，只靠父 hub 的 `CompanionSliceMode` 去裁 VFS / MCP / capability。
- 当前源码位置：`companion/tools.rs:400-407` 里甚至**直接传 `parent_context_bundle: None`**（真正继承发生在 `compose_companion_with_workflow` 路径）。普通 companion 分支 fragmment 继承实际上是空的，这是个**未被 slice 机制覆盖的差异**。

---

## 2. `contribute_*` 函数盘点与重叠分析

### 2.1 函数清单（file:line / 产出 slot / order）

| 函数 | 位置 | 适用 phase | 产出 slot / order |
|---|---|---|---|
| `contribute_core_context` | `context/builtins.rs:159` | task（compose_story_step） | `task`(10), `story`(20), `project`(40), `workspace`(50) |
| `contribute_binding_initial_context` | `context/builtins.rs:245` | task | `initial_context`(80) |
| `contribute_declared_sources` | `context/builtins.rs:262` | task（内部过滤掉 File/Snapshot） | 按 `ContextSlot` 映射（base_order=82）；warnings → `references`(89) |
| `contribute_instruction` | `context/builtins.rs:328` | task Start/Continue | `instruction`(90) Override；可选 `instruction`(100) Append |
| `contribute_mcp` | `context/builtins.rs:377` | task | `mcp_config`(85) + 附带 `RuntimeMcpServer` |
| `contribute_workspace_static_sources` | `context/workspace_sources.rs:19` | task（薄包装） | 透传调用方解析好的 fragment |
| `contribute_workflow_binding` | `context/workflow_bindings.rs:10` | task + owner（只在 compose_story_step 用） | `workflow_context`(83, 84.., 89) |
| `contribute_story_context` | `story/context_builder.rs:29` | owner(Story) | `story`(10), `project`(20), `workspace`(30, 来自 `workspace_context_fragment`), **+ SessionPlan fragments 内联**, declared sources（base_order=50）, workspace warnings(`story_context`, 59), 传入的 `workspace_source_fragments`, `story_context_warnings`(69) |
| `contribute_project_context` | `project/context_builder.rs:19` | owner(Project) | `project`(10), `project`(20 agent_identity), `workspace`(30), + SessionPlan fragments |
| `contribute_lifecycle_context`（本地） | `session/assembler.rs:1296` | lifecycle node | `workflow_context`(80, 83), `runtime_policy`(84) |
| `build_session_plan_fragments` | `session/plan.rs:93` | 共用 | `vfs`(35), `tools`(36), `persona`(37), `required_context`(38..), `workflow`(48), `runtime_policy`(49) |
| `hook_injection_to_fragment` / `From<&SessionHookSnapshot>` | `hooks/fragment_bridge.rs:36, 49` | Hook（尚未上线） | 按 `HOOK_SLOT_ORDERS`：`companion_agents`(60), `workflow`(83), `constraint`(84), 默认 200 |
| `build_continuation_bundle_from_markdown` | `context/builder.rs:138` | repository rehydrate | `static_fragment`(0) |
| `build_declared_source_warning_fragment` | `context/builder.rs:159` | 通用 helper | `references`(参数 order) |

### 2.2 重叠 / 漏洞清单

#### 2.2.1 `story` slot 在 task 路径下被渲染两次

Task 路径的 `contribute_core_context`（`builtins.rs:183`）产出 `story` slot order=20；同一路径没有 `contribute_story_context` 调用，但 `contribute_workflow_binding` 与 `contribute_instruction` 都会间接引用 `story.title`。重叠只是内容覆盖。但 owner(Story) 路径的 `contribute_story_context`（`story/context_builder.rs:32`）同样产出 `story` slot order=10，两套 slot 用的 `label`（`story_core`）一致但 `source` 不同（`legacy:contributor:core` vs `legacy:story_context`）。如果以后 session 同时走了两条路径（目前没有），会被 bundle 的 `Append` 合并，产生 content 重复。

#### 2.2.2 `workspace` slot 至少有三份实现

- `context/builtins.rs:213-239`（task 路径，含 `status` 字段，order=50）
- `context/builtins.rs:37 workspace_context_fragment`（owner 共享，order=30，不含 status）
- `story/context_builder.rs:60` / `project/context_builder.rs:59` 调用上面那个 helper

三份 label 都叫 `workspace_context`，但 order 分开（30 vs 50）+ 内容字段数不同，用户能从 bundle 里同时看到两条 `workspace` slot 的 fragment（如果 task 与 owner 路径都产出）——实际生产路径不会并发，但 audit bus 在同一 session 的不同 phase 会看到两种 shape，这给 Inspector 的 diff 体验添了摩擦。

#### 2.2.3 `workflow_context` slot 至少有三处产出

- `contribute_workflow_binding`（`workflow_bindings.rs:14, 57, 72`）：task 路径，渲染 `workflow_projection_snapshot` + 每个 resolved binding + warnings。
- `contribute_lifecycle_context`（`assembler.rs:1331, 1395`）：lifecycle node 路径，渲染 `lifecycle_node_context` + `lifecycle_workflow_injection`。
- `compose_companion_with_workflow` 内部手工 upsert（`assembler.rs:1630-1637`）：companion + workflow 路径，渲染 `companion_workflow_injection`。

三者的内容语义有重合（都包含 workflow goal / instructions / bindings），但没有共享 helper，**每次新增字段都要改三处**。

#### 2.2.4 `runtime_policy` slot 冲突

- `build_session_plan_fragments` 产出 `runtime_policy`(49)（`plan.rs:169`）。
- `contribute_lifecycle_context` 产出 `runtime_policy`(84)（`assembler.rs:1428`）。

在 lifecycle node 路径，如果以后把 SessionPlan 也进 contribution（目前 lifecycle node 不走 SessionPlan），两条 `runtime_policy` 会被 `upsert_by_slot` 按 `Append` 策略合并（`session_context_bundle.rs:64-72`），内容拼成一大段。order 字段只用于**排序**，不会影响合并语义。结果：用户看不到 SessionPlan 的 runtime_policy 和 lifecycle runtime policy 的边界。

#### 2.2.5 `workspace` 相关的 workspace declared sources 双路径

- `contribute_declared_sources`（`builtins.rs:274-284`）**过滤掉** `ContextSourceKind::File` / `ProjectSnapshot`，只走 `resolve_declared_sources`（`source_resolver.rs:45`）即 `ManualTextResolver` 等。
- `workspace_sources.rs::resolve_workspace_declared_sources`（`workspace_sources.rs:27`）**只处理** File / ProjectSnapshot 两种 kind，需要 VFS 在线。

所以对同一份 `ContextSourceRef` 列表，实际上有两个不同的入口函数，依据 kind 分流。它们的 fragment 构造 helper（`fragment_slot` / `fragment_label` / `render_source_section` / `display_source_label`）**逐行重复**在两个文件（`source_resolver.rs:127-155` vs `workspace_sources.rs:359-387`）。

#### 2.2.6 `SessionPlan` fragment 的 **嵌入 vs 外挂** 不统一

- Owner 路径：`contribute_story_context` / `contribute_project_context` **内部直接调用** `build_session_plan_fragments` 并 `extend` 到自己的 fragment 里（`story/context_builder.rs:65-80` / `project/context_builder.rs:63-77`）。最终用一个 Contribution 打包。
- Task 路径：`compose_story_step` **在外部** 独立调用 `build_session_plan_fragments`，然后 `contributions.push(Contribution::fragments_only(session_plan.fragments))`（`assembler.rs:1099-1132`）。
- Lifecycle 路径：**完全不调用** SessionPlan。

副作用：Inspector 看到的 audit 事件里，`source` 字段不一致：
- owner 路径走 `legacy:story_context` / `legacy:project_context`（因为 fragment 是 contributor 内部 push 出来的，没覆盖 source）—— **实际代码里 SessionPlan fragments 的 source 是 `legacy:session_plan`，不会被 contributor 改写，因为 `fragments.extend(session_plan.fragments)` 直接搬结构体**，但 contributor 的 `Contribution.fragments` 里就会同时出现 `legacy:session_plan` 和 `legacy:story_context` 两种 source；
- task 路径同样出现 `legacy:session_plan`。

source 一致，但**没有对应的一级 trigger**。

### 2.3 漏洞

- `contribute_core_context` 产出 `story` slot，但 task 路径后续并没有 story 层 declared sources 之类的增量 —— owner `contribute_story_context` 能产出的 story 级 SessionPlan、declared sources、workspace fragments 在 task 路径里**全部由 `compose_story_step` 内联重做**，没有复用 `contribute_story_context`。如果后续 story 领域新增一条 fragment 产出，task 路径**不会自动继承**。
- `contribute_instruction` 用 `MergeStrategy::Override`，意味着如果同一 session 同 slot 的更早 fragment 已经存在（例如从 hook 注入或 lifecycle runtime），会被**直接替换**。hook_delegate 目前没有往 `instruction` slot 写 fragment，风险暂时是理论的。
- 没有任何 `contribute_capability` / `contribute_flow_capabilities`。能力集合完全走 `FlowCapabilities` 独立字段，而不是进入 bundle；这是明显的"第二主数据面"。

---

## 3. Augmenter / SystemPromptAssembler 的职责

### 3.1 PromptRequestAugmenter trait（augmenter.rs）

- 定义：`crates/agentdash-application/src/session/augmenter.rs:20`。
- 职责：在 `SessionHub` 内部需要构造 `PromptSessionRequest`（hook auto-resume 场景）时，先调用 augmenter 把"裸"请求补齐 MCP/VFS/FlowCaps/ContextBundle/BootstrapAction/CapabilityKeys 等。
- 实现：**API 层唯一实现**（见 `crates/agentdash-api/src/app_state.rs:*`，grep `PromptRequestAugmenter` 看到 augmenter 在 AppState 初始化时注入）。注入后 augmenter 实际走的是上面 §1 的 4 个 compose 分支，再把 `PreparedSessionInputs` finalize 回 `PromptSessionRequest`。
- 与 Contribution 模型**不冲突**：augmenter 是**入口路径适配器**，调用 compose_* 再 finalize，不会重写 bundle 合并策略。但它的存在把"bundle 装配"从业务入口（HTTP handler / task service / hook auto-resume）拉到了一个"暗路径"，Inspector 侧要识别 augmenter 产出要依赖 `audit_session_key` 是否正确传入。

### 3.2 SystemPromptAssembler（system_prompt_assembler.rs）

- 定义：`crates/agentdash-application/src/session/system_prompt_assembler.rs:22-36`。
- 职责：**Application 层**最终组装四层 Identity Pipeline 的 system prompt 文本，Connector 只收到 `String`。
- 输入：`SystemPromptInput { base_system_prompt, agent_system_prompt, user_preferences, discovered_guidelines, context_bundle, session_capabilities, vfs, working_directory, runtime_tools, mcp_servers, hook_session }`。
- 渲染顺序（`assemble_system_prompt`，line 39）：
  1. Identity（base + agent sp + user prefs）
  2. Project Guidelines（AGENTS.md / MEMORY.md）
  3. Project Context（`bundle.render_section(RuntimeAgent, RUNTIME_AGENT_CONTEXT_SLOTS)`）
  4. Companion Agents（`session_capabilities.companion_agents`）
  5. Workspace（`vfs.mounts` 概览）
  6. Available Tools（runtime_tools + mcp_servers）
  7. Hooks（`build_hook_runtime_sections`）
  8. Skills（`format_skills_from_capabilities`）
- 与 Contribution 模型的关系：
  - **不重复**：bundle.render_section 只产出 "## Project Context" 这一个 section，其他 section（Identity/Guidelines/Workspace/Tools/Hooks/Skills）都**不走 bundle**，而是直接从独立字段渲染。
  - **潜在冗余**：
    - `Workspace`（sp 第 5 段）和 `contribute_core_context` / `workspace_context_fragment` / SessionPlan `vfs` slot **都** 描述 VFS mount，但视角不同：sp 里是"以 mount id/display_name 为核心的简表"，bundle 里的 `vfs` slot 是"markdown 摘要 + usage 提示"，`workspace` slot 是"workspace.id / binding / status"——三份内容不重复但协同语义不清。
    - `Companion Agents`（sp 第 4 段）由 `session_capabilities.companion_agents` 渲染；而 `companion_agents` 这个**同名 slot** 也出现在 hook injection fragment order 映射里（`fragment_bridge.rs:17`）——两条路径对同一概念用不同数据面。
    - `Hooks`（sp 第 7 段）显式说 "动态治理信息会在每次 LLM 调用边界由 runtime 注入；这里不再重复展开它们的静态副本"（`system_prompt_assembler.rs:367`），这是 Hook 与 Bundle 边界的**唯一明文声明**。
- 结论：SystemPromptAssembler **不是另一个 augmenter**，而是 bundle 下游**唯一**的渲染入口；它和 Contribution 模型是串联关系，不是竞争。

---

## 4. Hook fragment_bridge 的实际接线状态

### 4.1 定义（fragment_bridge.rs）

- `hook_injection_to_fragment(injection: HookInjection) -> ContextFragment`：按 slot 映射 order（`companion_agents`→60, `workflow`→83, `constraint`→84, 其余→200），scope 用默认（`RuntimeAgent | Audit`）。
- `impl From<&SessionHookSnapshot> for Contribution`：把 snapshot 的所有 injections 转换成 fragment-only Contribution。

### 4.2 运行时调用点

- `crates/agentdash-application/src/session/hook_delegate.rs:17`：`use crate::hooks::hook_injection_to_fragment;`（可见导入存在）。
- `hook_delegate.rs:113`：`emit_hook_injection_fragments` 内部**用 `hook_injection_to_fragment` 转换后，调 `emit_fragment` 写入审计总线**（不是写入 Bundle）：
  ```
  for injection in injections.iter().cloned() {
      let fragment = hook_injection_to_fragment(injection);
      emit_fragment(bus.as_ref(), bundle_id, session_id, bundle_session_uuid,
                    AuditTrigger::HookInjection { trigger: trigger_label.clone() },
                    &fragment);
  }
  ```
- 这个 `emit_hook_injection_fragments` 在三个 trigger 调用：
  - `transform_context` → `HookTrigger::UserPromptSubmit`（`hook_delegate.rs:335`）
  - `after_turn` → `HookTrigger::AfterTurn`（`hook_delegate.rs:545`）
  - `before_stop` → `HookTrigger::BeforeStop`（`hook_delegate.rs:604`）

### 4.3 `impl From<&SessionHookSnapshot> for Contribution` 的运行时调用点

**零。** grep 显示除了 `fragment_bridge.rs` 自身的单测（`snapshot_injections_map_to_contribution`），没有任何生产代码使用该 impl。

### 4.4 User Message 渲染路径（仍是主出口）

- `hook_delegate.rs:395-401`：`transform_context` 内部把 `resolution.injections` 通过 `build_hook_injection_message` 拼成一条 `AgentMessage::user(...)` append 到 `messages`。
- `hook_delegate.rs:808`：`build_hook_injection_message` 内部用 `HOOK_USER_MESSAGE_SKIP_SLOTS = &["companion_agents"]`（`hook_delegate.rs:806`）过滤掉 `companion_agents` slot。
- `after_turn` / `before_stop`：`build_hook_steering_messages` 走**不过滤**的 `build_hook_markdown`（`hook_delegate.rs:827`）—— steering 路径 companion_agents slot 如果出现会**重复进入 user message**。

### 4.5 结论

- **bridge 已半接入运行时**：审计总线能看到 `hook:UserPromptSubmit` / `hook:AfterTurn` / `hook:BeforeStop` 事件（对应 Inspector 能显示）。
- **bridge 尚未接入 Bundle**：`SessionContextBundle` 本身**不会**被 hook 链路追加新的 fragment；PiAgent 的 system prompt 每次读到的是 bootstrap 时的 bundle 快照。
- **User message 通道仍是 hook 注入主出口**：`HOOK_USER_MESSAGE_SKIP_SLOTS` 里只列了 `companion_agents` 一个 slot；更极端的重复去重靠 `SessionContextBundle::upsert_by_slot` —— 但 bundle 根本没被合并进来，所以白名单**承担了主去重职责**，与 PRD "降级为过渡保护" 目标相反。

---

## 5. Prompt blocks 通道（compose_* → prompt_blocks → user message）

### 5.1 compose_* 的 prompt_blocks 行为

| compose | prompt_blocks 来源 | 是否注入 owner context | 备注 |
|---|---|---|---|
| `compose_owner_bootstrap` | 透传 `spec.user_prompt_blocks`（`assembler.rs:847`） | 不注入 | bundle 单独进 system prompt |
| `compose_story_step` | 固定 `build_story_step_trigger_prompt_blocks(phase)`（`assembler.rs:1153`、常量文本 "请开始执行当前任务。" / "请继续推进当前任务。"） | 不注入 owner context；完全丢弃 `spec.override_prompt` / `spec.additional_prompt` 已经在 bundle 里用 `instruction` slot 承载 | 2026-04-30 的 guard 实现 |
| `compose_lifecycle_node` | 固定 "请执行当前 lifecycle 节点。"（`assembler.rs:398` via `apply_lifecycle_activation`） | 不注入 | |
| `compose_companion` / `compose_companion_with_workflow` | `dispatch_prompt` 文本（`assembler.rs:1653`、`apply_companion_slice`） | 父 bundle 通过 builder 继承 | `bootstrap_action = OwnerContext`，但不是 owner bootstrap（见 §5.2） |

### 5.2 `SessionBootstrapAction::OwnerContext` 的语义消费点

grep 结果显示 `OwnerContext` 被判定的唯一位置：
- `prompt_pipeline.rs:105`：`let is_owner_bootstrap = req.bootstrap_action == SessionBootstrapAction::OwnerContext;`

用法分支：
- `prompt_pipeline.rs:117-152`：决定 hook_session 的 load 方式（owner_bootstrap=全量 reload；否则复用 existing）。
- `prompt_pipeline.rs:337-339`：写 `session_meta.bootstrap_state = SessionBootstrapState::Bootstrapped`。
- `prompt_pipeline.rs:379-397`：**注入 `agentdash://session-capabilities/{session_id}` resource block 到 user_blocks 最前面**（只在 `is_first_prompt || is_owner_bootstrap` 时发生）。
- `prompt_pipeline.rs:420-445`：触发 `HookTrigger::SessionStart`。

所以 `OwnerContext` 动作的真实业务语义是：
1. 决定是否 reload hook snapshot + 触发 SessionStart；
2. 决定是否 **在 user_blocks 首部塞一个 session-capabilities resource block**（这是**唯一**仍"由 bundle 之外的路径往 user message 里塞结构化上下文"的生产路径）。
3. 把 `bootstrap_state` 标成已完成。

**潜在冗余 / 耦合信号**：
- `companion_agents` 既在 `session_capabilities.companion_agents`（进 system prompt `## Companion Agents` section），又在 session-capabilities resource block（以 JSON 形式塞进 user_blocks 首部），**同一数据面渲染两次**，只有依据不同（一个纯 SP，一个 user message）。
- Companion + workflow 路径（`compose_companion_with_workflow`）也设 `bootstrap_action = OwnerContext`（`assembler.rs:1669`），所以 companion session 的首轮 prompt 也会塞一个 session-capabilities resource block。这是否预期语义 PRD 未明确。

### 5.3 `render_section` 调用点（bundle → markdown）

`SessionContextBundle::render_section` 的生产调用点：
- `system_prompt_assembler.rs:83-89`：主路径，进 `## Project Context` section。
- `routes/acp_sessions.rs:1448-1453`：task continuation 路径回灌 markdown（见 §1.2c）。
- 测试里的 render 不计入。

所以主路径 **已经** 只有 system prompt 用 bundle → markdown；唯一的 "本该只进 system prompt 的内容重复进 user message" 路径是 task continuation 里 `task_context_markdown` → `build_continuation_bundle_from_markdown` → `context_bundle`，而不是 user message。PRD 的 "双重注入" 描述在当前代码里已经不准确。**真正残留的双重注入只存在于 `session-capabilities` 的 SP + user_blocks 两条路径**。

---

## 6. mount_file_discovery / source_resolver / workspace_sources / vfs_discovery 边界

### 6.1 mount_file_discovery（context/mount_file_discovery.rs）

- 职责：按规则（`MountFileDiscoveryRule`）**遍历已挂载的 VFS mount，读取约定文件内容**（AGENTS.md / MEMORY.md / SKILL.md），返回内容+诊断。
- 规则：
  - `BUILTIN_GUIDELINE_RULES`（line 65）：`agents_md` + `memory_md`，扫根 + 一级子目录。
  - `BUILTIN_SKILL_RULES`（line 84）：`skill_md`，扫 `.agents/skills/*/` 和 `skills/*/`。
- 调用点：
  - `session/prompt_pipeline.rs:234`：每次 prompt 都用 `BUILTIN_GUIDELINE_RULES` 扫一遍，结果 → `discovered_guidelines` → system prompt `## Project Guidelines`。
  - `skill/loader.rs:106`：`load_skills_from_vfs` 用 `BUILTIN_SKILL_RULES` 扫，结果 → `session_capabilities.skills`。
- **不走 Bundle**，完全独立。

### 6.2 source_resolver（context/source_resolver.rs）

- 职责：按 `ContextSourceKind` 调度已注册的 `SourceResolver` 实现，把 `ContextSourceRef`（用户/story/task 声明的上下文来源）解析为 `ContextFragment`。
- 内置实现：`ManualTextResolver`（line 107）。
- 注册：`SourceResolverRegistry::with_builtins()`（line 16）只注册 `ManualText`。其他 kind（`HttpFetch` / `McpResource` / `EntityRef` / `File` / `ProjectSnapshot`）全部走 "未注册 resolver" 分支，产生 warning 或 `InjectionError`。
- 导出入口：`resolve_declared_sources(request)`（line 45）/ `resolve_declared_sources_with_registry(request, registry)`（line 52）。
- 调用点：
  - `context/builtins.rs:285` (`contribute_declared_sources` 内，**过滤掉 File/ProjectSnapshot 后调用**)。
  - `story/context_builder.rs:96`（`contribute_story_context` 内，同上过滤）。

### 6.3 workspace_sources（context/workspace_sources.rs）

- 职责：**专门处理 `ContextSourceKind::File` / `ProjectSnapshot`**，需要 VFS + BackendAvailability 才能工作。
- 导出入口：
  - `resolve_workspace_declared_sources(availability, vfs_service, sources, workspace, base_order)`（line 27）—— 异步、需要 workspace 在线。
  - `contribute_workspace_static_sources(fragments)`（line 19）—— 薄包装：把已解析的 fragment 列表塞进 `Contribution`，供 compose 组装。
- 调用点：
  - `session/assembler.rs:797`（`compose_owner_bootstrap` Story 分支）
  - `session/assembler.rs:1039`（`compose_story_step`）
- **和 source_resolver 的 fragment helper 完全重复**：`fragment_slot` / `fragment_label` / `render_source_section` / `display_source_label` / `truncate_text` 四个私有函数两处独立实现（`workspace_sources.rs:359-387` vs `source_resolver.rs:127-155`）。

### 6.4 vfs_discovery（context/vfs_discovery.rs）

- 职责：**对外广播"当前 session 能发现哪些上下文来源空间"的描述符**，供前端 UI 用。
- 核心类型：`VfsDescriptor`（label / provider / supports / selector）。
- 内置 Provider：`WorkspaceFileProvider`、`WorkspaceSnapshotProvider`、`McpResourceProvider`、`LifecycleVfsProvider`。
- 调用点：grep 显示 `builtin_vfs_registry()` 的运行时调用点在 API 层（估计是 context source 的发现 endpoint），**完全不进入 Bundle**，也不在 session assembler 路径出现。

### 6.5 四者差异总结

| 模块 | 输入 | 输出 | 消费方 | 是否进 Bundle |
|---|---|---|---|---|
| mount_file_discovery | VFS + 规则（文件名） | 文件内容 | system prompt `## Project Guidelines`、skill loader | 否 |
| source_resolver | `ContextSourceRef`（非 workspace kind） | `ContextFragment` | `contribute_declared_sources` / `contribute_story_context` | 是（经 Contribution） |
| workspace_sources | `ContextSourceRef`（File/Snapshot） + VFS | `ContextFragment` | `compose_story_step` / `compose_owner_bootstrap` 直接传入 | 是（经 Contribution） |
| vfs_discovery | VfsContext 开关 | `VfsDescriptor` JSON | 前端 UI selector | 否 |

**"实际上干同一件事"**：仅 `source_resolver` 与 `workspace_sources` 是同一件事被按 kind 劈开，fragment helper 重复。其他两个是完全独立的关注点。

---

## 7. 冗余与耦合信号汇总

### 7.1 跨 crate 硬编码常量

- `RUNTIME_AGENT_CONTEXT_SLOTS`（`agentdash-spi::context_injection:11`）：**单点真相**，被 `system_prompt_assembler.rs:85` 和 `routes/acp_sessions.rs:1451` 共享。PRD "白名单散落三处" 的描述与实际不符 —— `agentdash-api` 只是 import，没有自己维护一份。
- `HOOK_USER_MESSAGE_SKIP_SLOTS`（`hook_delegate.rs:806`）：**独立常量**，只包含 `companion_agents`。与 `RUNTIME_AGENT_CONTEXT_SLOTS` 不冗余，但职责是"bundle 合并没接入时 user message 的防重复"。
- `HOOK_SLOT_ORDERS`（`fragment_bridge.rs:15`）：hook → order 映射，硬编码 `companion_agents`(60), `workflow`(83), `constraint`(84)。与 contributor 里的硬编码 order 数字（`workflow_context`(83) 等）**隐式绑定**：如果 `contribute_workflow_binding` 的 order 改成 85，hook 的 `workflow` slot fragment 仍然是 83，会插在 contributor 的前面。**没有共享 constant 或 enum**。
- `contribute_*` 里散落的 order 常量：10/20/30/35/36/37/38/40/48/49/50/60/80/82/83/84/85/86/89/90/96/100/200。这些数字决定了 bundle 内 fragment 的相对位置，但**没有任何中央注册处**。

### 7.2 同一概念多次拷贝 / 不同字段名

- `workspace` 信息：`Workspace.identity_kind` / `Workspace.status` 在 `contribute_core_context`（task 路径）里出现；`workspace_context_fragment`（owner 共享）不含 status；`build_workspace_snapshot_from_entries`（workspace_sources.rs:256）又另写一份"项目快照 + tech_stack"的 workspace 视图。
- `workflow goal/instructions`：
  - `contribute_workflow_binding` 渲染 "Workflow Projection Snapshot"（metadata only）+ 每个 binding + warnings。
  - `contribute_lifecycle_context` 渲染 "Workflow Goal" + "Workflow Instructions" + "Workflow Context Bindings"（全文本）。
  - `compose_companion_with_workflow` 手工拼 "Workflow Goal" + "Workflow Instructions"（不包含 bindings）。
- `mcp_server` 相关：
  - ACP `agent_client_protocol::McpServer` ↔ `RuntimeMcpServer`（application 内部抽象）之间有 `acp_mcp_servers_to_runtime` / `runtime_bridge::acp_mcp_server_to_runtime` / `runtime_mcp_servers_to_acp` 三个转换函数分散在 assembler.rs / runtime_bridge.rs / context/builtins.rs。
- `companion_agents` 概念：
  - 作为 `SessionBaselineCapabilities.companion_agents` 字段（SP 独立 section）
  - 作为 `HookInjection { slot: "companion_agents" }`（hook injection → fragment）
  - 作为 `agentdash://session-capabilities/{session_id}` resource block（user message 注入）
  同一份数据三条路径。

### 7.3 其他耦合点

- **Story step task 路径不调用 owner story contributor**。Task 路径的 `compose_story_step` 没有调用 `contribute_story_context`（只调 `contribute_core_context`），意味着 Story 层面的 SessionPlan、declared sources、workspace fragments 全部由 task 路径**逐条重手写**。如果 story contributor 以后增加 field，task 路径不会自动跟进，是明确的 drift 风险。
- **lifecycle node path 不走 SessionPlan**。`compose_lifecycle_node_with_audit` 完全没有 `build_session_plan_fragments` 调用，VFS / tools / persona / runtime_policy 摘要都缺失——只能靠 `contribute_lifecycle_context` 里自己写的 `runtime_policy` 单条 fragment 撑住。对 lifecycle agent 来说 tool 列表只能依赖 system prompt 的 `## Workspace` + `## Available Tools` section，从 bundle 角度看**lifecycle bundle 是最薄的**。
- **companion slice 未对 bundle 做裁剪**。`CompanionSliceMode`（Full/Compact/WorkflowOnly/ConstraintsOnly）只影响 `build_companion_execution_slice`（VFS/MCP/能力裁剪），**没有对继承的 `parent_context_bundle` 做 fragment 级过滤**。意味着 `ConstraintsOnly` 的 companion 仍会看到父 session 的 `task` / `story` / `project` fragment 全文。
- **Audit session key 透传依赖调用方**。`compose_owner_bootstrap` / `compose_story_step` 的 spec 都要求调用方传 `audit_session_key`。在 `compose_lifecycle_node` 的两种 entrypoint 里（`SessionRequestAssembler::compose_lifecycle_node` 和自由函数 `compose_lifecycle_node`），audit_bus + audit_session_key 是分离参数（`assembler.rs:1230-1236`），不在 Spec 里。在 routine / orchestrator 路径容易漏传，导致 bundle emit 但 audit 没收录。

### 7.4 数据流图（简化）

```
USER HTTP/routine/hook-auto-resume
        │
        ├─► build_{task|story|project|project_agent}_prompt_request (routes/routine)
        │        │
        │        ▼
        │   SessionRequestAssembler::compose_{owner_bootstrap|story_step|lifecycle_node|companion[_with_workflow]}
        │        │
        │        ├─► contribute_core/story/project/binding/declared/mcp/workflow_binding/instruction/lifecycle_context
        │        │        │
        │        │        ├─► resolve_declared_sources (source_resolver.rs)
        │        │        ├─► resolve_workspace_declared_sources (workspace_sources.rs)
        │        │        └─► build_session_plan_fragments (plan.rs)
        │        │
        │        ├─► build_session_context_bundle (reducer, 无 domain 依赖)
        │        │
        │        └─► ContextAuditBus::emit  (AuditTrigger::SessionBootstrap | ComposerRebuild)
        │
        ▼
PreparedSessionInputs → finalize_request → PromptSessionRequest
        │
        ├─► context_bundle ──► SessionHub::start_prompt_with_follow_up
        │                           │
        │                           ├─► mount_file_discovery (AGENTS.md/MEMORY.md) → discovered_guidelines
        │                           ├─► load_skills_from_vfs  (skill/loader) → session_capabilities.skills
        │                           ├─► build_session_baseline_capabilities (companion_agents from hook snapshot)
        │                           ├─► SystemPromptInput { context_bundle, guidelines, capabilities, vfs, hook_session, ... }
        │                           ├─► assemble_system_prompt → ExecutionContext.assembled_system_prompt
        │                           ├─► build_tools_for_execution_context → ExecutionContext.assembled_tools
        │                           ├─► [if owner_bootstrap] inject session-capabilities resource block in user_blocks (prompt_pipeline.rs:379-397)
        │                           └─► connector.prompt (PiAgent connector)
        │
        └─► hook delegate HookRuntimeDelegate
                ├─► transform_context → UserPromptSubmit hook
                │       ├─► emit_hook_injection_fragments (只 emit audit, 不 merge bundle)
                │       ├─► build_hook_injection_message (走 HOOK_USER_MESSAGE_SKIP_SLOTS 去重) → user message
                │       └─► 【尚未】把 hook_injection_to_fragment 产物 merge 到 bundle
                ├─► after_turn / before_stop 同上
                └─► after_compaction / before_provider_request 等（未处理 fragment 语义）
```

---

## 8. 结论与未列入 PRD 的额外信号

PRD 里已经列出的四个未闭环点：

1. Task owner bootstrap 把 bundle 渲染为 prompt resource block。**现状**：OwnerBootstrap 路径已经干净（不 prepend 了）；只剩 task continuation 的 `static_fragment` 合并点和 `session-capabilities` resource block 这两个残留。PRD 这条描述已经部分过期。
2. `compose_lifecycle_node` 不产 Bundle。**现状**：已经产了，但 `kickoff_prompt` 被拍扁到 `runtime_policy` slot；尚缺 `AuditTrigger::LifecycleActivation` 细分。PRD 描述滞后。
3. Hook 动态注入仍以 user message 渲染，fragment_bridge 未接入运行时。**现状**：bridge 已接审计 emit，但**没接 bundle merge**；`HOOK_USER_MESSAGE_SKIP_SLOTS` 仍是主去重。PRD 描述基本准确。
4. Slot 白名单分散在 application / PiAgent / Vibe Kanban 三处。**现状**：`RUNTIME_AGENT_CONTEXT_SLOTS` 已单点维护；PiAgent / Vibe Kanban 未另立白名单（至少在当前 workspace 的 crate 里搜不到）。PRD 这条描述与实际不符。

**PRD 没列出但可能需要关注的冗余/耦合**：

- `source_resolver` 与 `workspace_sources` 的 fragment helper 代码重复（§6.2/6.4/6.5）。
- `workflow_context` slot 的三份渲染实现（§2.2.3 / §6.3）。
- `workspace` slot 至少三份实现（§2.2.2）。
- SessionPlan "嵌入式" vs "外挂式" 调用不统一（§2.2.6）。
- Owner path vs task path 缺少 `contribute_story_context` 复用（§7.3）。
- Lifecycle path 完全不走 SessionPlan，bundle 最薄（§7.3）。
- Companion slice_mode 对 bundle 无裁剪（§7.3）。
- `companion_agents` 的三条路径（SP section + session-capabilities resource block + HookInjection slot）（§7.2）。
- `compose_companion_with_workflow` 手工 upsert bundle，不过审计总线（§1.4）。
- `HOOK_SLOT_ORDERS` 与 contributor order 数字隐式绑定，无共享常量（§7.1）。
- `audit_session_key` 透传耦合调用方（§7.3）。

---

## Caveats / Not Found

- 未系统审阅 PiAgent connector 内部是否另行读取 bundle（grep 显示 `agentdash-executor` 内**没有** `SessionContextBundle` / `context_bundle` / `RUNTIME_AGENT_CONTEXT` 字符串）。`system_prompt_assembler` 在 application 层已经把 bundle → string，connector 只拿到 `ExecutionContext.assembled_system_prompt: Option<String>`。所以 PRD 里 "PiAgent `build_runtime_system_prompt` 把业务上下文读取切到 context_bundle" 这段描述对应的是 application 层的 assembler，而不是 pi_agent crate 内部。
- 未深入 Vibe Kanban 连接器（grep 未找到它直接读 bundle 的地方；不保证它没有本地复制 slot 白名单）。
- 未核对 `agentdash-api` 的 context audit route DTO 是否真的能显示 lifecycle / hook 事件；只确认了审计总线侧 emit 存在。
- 未评估 `build_declared_source_warning_fragment`（builder.rs:159）与 `contribute_declared_sources` 内部同样 warning 段（builtins.rs:292）在 task 路径是否会产生双重 warnings —— 如果 `resolve_declared_sources` 与 `resolve_workspace_declared_sources` 都有 warnings，会被 push 成**两条** warning fragment（assembler.rs:1069 + builtins.rs:293），slot 都是 `references`、order 分别是 96 和 89，会被 `upsert_by_slot` 合并成同一 fragment 的 `\n\n` 拼接。需单测验证。
- `ContextContributor` trait 在源码中是否还有引用未确认（PRD 说"保留作为可选 plugin 扩展点"）—— grep 未找到定义，有可能已经删除。

