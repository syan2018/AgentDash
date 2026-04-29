# 统一 Session 上下文注入流水线（ContextBuilder / D2b 方案）

## Goal

为 AgentDashboard 中"一个 session 从创建到每一轮用户消息"之间所有上下文注入点，引入统一的 **SessionContextBundle** 数据面，让 Fragment 从"中段结构、终段字符串"变为"**贯穿数据流终点的结构化数据**"。

目标收益：
1. **消除浆糊**：Compose 链路（Contributor/SessionPlan）与 Hook 链路共享同一数据结构（`ContextFragment`），删除 `SESSION_BASELINE_INJECTION_SLOTS` / `filter_user_prompt_injections` 这类硬编码去重
2. **可观测**：任何 Fragment 进入 Bundle 都有审计事件，前端 Context Inspector 能回放一个 session 完整的上下文注入时间线
3. **scope 级隔离**：title generator / summarizer / bridge replay 按 `FragmentScope` 过滤而非靠人肉约定（从协议层加固 `bce0825` 修复）
4. **构建范式统一 + 依赖方向修正**：`context/builder` 不再 reach into domain，变为纯合并 reducer；领域自治暴露 `contribute_*() -> Contribution`（与 `build_session_plan_fragments` 同构）；调用方按 phase 组装 `Vec<Contribution>`。`extra_contributors: Vec<Box<dyn>>` 这种"用 trait 伪装循环 + 反向依赖 domain"的双重反模式彻底清理
5. **为 D2a 留口**：Fragment 数据模型已对齐，未来可演进为 Hook 执行路径彻底合并的终极形态

## 背景

调研基础：[research/context-injection-map.md](research/context-injection-map.md)（约 40 个注入点，8 个时机分组）。

**浆糊的精确结构**：

```rust
// agentdash-spi/src/context_injection.rs:31
pub struct ContextFragment {
    pub slot: &'static str,  // ← 封闭集合
    pub label: &'static str,
    pub order: i32,
    pub strategy: MergeStrategy,
    pub content: String,
    // 缺 source / scope
}

// agentdash-spi/src/hooks.rs:53
pub struct HookInjection {
    pub slot: String,  // ← 动态字符串
    pub content: String,
    pub source: String,
    // 缺 order / strategy / label
}
```

两个结构做的是同一件事，字段互补残缺。Fragment 又在 [ContextComposer](../../../crates/agentdash-application/src/context/composer.rs) 那一步被压成 `system_context: String`，下游（[pi_agent/connector.rs:241-422 build_runtime_system_prompt](../../../crates/agentdash-executor/src/connectors/pi_agent/connector.rs#L241-L422)）只能整段塞入 `## Project Context`，失去所有 slot/order/source 元信息。

项目**其实已有良好的工厂模式模板** —— [session/plan.rs build_session_plan_fragments](../../../crates/agentdash-application/src/session/plan.rs)（`SessionPlanInput → Vec<ContextFragment>`），只是应用面太窄，且输出被过早 render。本任务就是把这个模式推到终点。

## 最终决策（brainstorm 收敛）

| # | 决策 | 选择 |
|---|------|------|
| D1 | ContextFragment 字段扩展 | **全加**：`scope: FragmentScopeSet` + `source: String`；`slot: &'static str → String` |
| D2 | Hook 链路接入方式 | **D2b**：Hook 生产者改产 `ContextFragment`，但保留在 agent_loop 每轮动态产出的执行路径；`build_hook_injection_message` 退化为薄渲染层 |
| D3 | ExecutionContext 双字段过渡 | 保留 `system_context: Option<String>`，并列引入 `context_bundle: Option<SessionContextBundle>`；F1 优先读 bundle、fallback 到 String |
| D4 | 本任务切片 | **后端完整切 + 前端 Inspector 面板** |
| D5 | 替换实验 API | **砍掉** — 对照实验还要扣模型随机性，投入产出不成比例 |
| D6 | 构建范式 | **依赖倒置 + 数据驱动 reducer**：`build_session_context_bundle(config, contributions) -> Bundle` 是纯合并函数，**不依赖任何 domain 类型**。每个领域自治暴露 `contribute_*() -> Contribution` 纯函数（与 `build_session_plan_fragments` 同构），调用方负责聚合成 `Vec<Contribution>` 喂给 builder。`ContextContributor` trait 保留作为可选 plugin 扩展点但不再是主力 |

## Non-Goals（本任务明确不做）

- **D2a 激进合并**：Hook 执行路径彻底并入 Bundle 主通道，产出/副作用拆分为 `Fragment + TurnEffect`。存档见姊妹任务 `session-context-builder-d2a-exploration`
- **Compact 机制 Fragment 化**：`AgentMessage::CompactionSummary` 和 `compaction/mod.rs` 的 messages 列表重写逻辑保持现状。D2a 演进时可收敛为"Static Fragment + message tombstone"
- **Hook 副作用类动作重构**：block / transform_message / deny tool / ask approval / abort 继续走现有 `HookResolution` 路径
- **B13 prompt_blocks[0] SessionCapabilities resource block 清理**：等 F1 Bundle 消费稳定后单独收尾任务处理
- **其他 executor（Vibe Kanban / 未来 Codex）迁移 Bundle 消费**：跟进任务
- **AGENTS.md / CLAUDE.md 自动加载**：姊妹任务 `04-29-agents-md-discovery-loading`，接 Fragment 的 `source` 字段
- **Fragment 级别 UI 编辑/禁用 / context_overrides API**：D5 砍掉
- **`system_context: String` 字段最终删除**：双写几个 sprint 后的 cleanup 任务
- **其他 Contributor 内部逻辑**：不重写任何现有 contributor 的产出内容（仅搬家 trait impl → 纯函数，Markdown 输出严格保持一致）
- **Hook 生命周期语义**：不引入 `FragmentLifetime`（`Static/PerTurn/SingleUse/Ephemeral`），保持每轮重算的现状
- **ContextContributor trait 最终删除**：trait 保留作为 plugin 扩展点，`04-12-plugin-extension-api` 推进时再决定是否统一为 `FragmentProducer`

## Acceptance Criteria

### 后端
- [ ] `ContextFragment` 含 `scope: FragmentScopeSet` + `source: String`；`slot: String`
- [ ] `SessionContextBundle` 类型存在，可持有 `Vec<ContextFragment>` + `bundle_id` + `session_id`
- [ ] `ExecutionContext` 持有 `context_bundle: Option<SessionContextBundle>` 字段（`system_context: Option<String>` 并行保留）
- [ ] `build_session_context_bundle(config, contributions) -> SessionContextBundle` 函数存在；`context/builder.rs` 模块**不引用任何 domain 类型**（task/story/project/workflow 等）
- [ ] `Contribution { fragments, mcp_servers }` 类型存在，作为所有领域向 builder 投递的标准契约
- [ ] 4 个原 builtin contributor（Core/Binding/DeclaredSources/Instruction）迁移为各领域模块自暴露的 `contribute_*` 纯函数
- [ ] MCP / Workflow Bindings / WorkspaceSources 也暴露 `contribute_*` 纯函数；原 `McpContextContributor` / `WorkflowContextBindingsContributor` / `StaticFragmentsContributor` 的 trait impl 删除
- [ ] `ContextContributorRegistry::with_builtins()` 删除；`Vec<Box<dyn ContextContributor>>` 形态的 `extra_contributors` 从 assembler.rs 移除，调用方改为组装 `Vec<Contribution>`
- [ ] `ContextContributor` trait 保留但不再是主力；未来插件扩展点由 `04-12-plugin-extension-api` 决定最终形态
- [ ] `SessionAssemblyBuilder` 新增 `with_context_bundle(bundle)` 方法作为主力入口；`with_system_context(String)` 标 `#[deprecated]` 保留为过渡期兜底；其他链式方法（`with_vfs` / `append_lifecycle_mount` / `with_resolved_capabilities` 等）不改
- [ ] 4 个 `compose_*` 方法（`compose_owner_bootstrap` / `compose_story_step` / `compose_lifecycle_node` / `compose_companion`）改用 `.with_context_bundle(build_session_context_bundle(config, contributions))` 替代原 `.with_system_context(markdown)`
- [ ] OwnerBootstrap 链路（`compose_owner_bootstrap` / `compose_story_step` / `compose_lifecycle_node`）改调 `build_session_context_bundle`，Bundle 成为 F1 的主数据源
- [ ] 旧路径 String fallback 仅作为过渡期兜底，工厂必须无条件产出 Bundle
- [ ] `SESSION_BASELINE_INJECTION_SLOTS` 常量删除
- [ ] `filter_user_prompt_injections` 函数删除，去重由 Bundle 的 slot 命名空间唯一性保证
- [ ] Hook 产出的 `HookInjection` 在汇入 Bundle 前被转换为 `ContextFragment`（带 `source` / `scope` / `order` 默认值）
- [ ] `build_hook_injection_message` 输入改为 `&[ContextFragment]`，行为与现有 user message 输出保持一致
- [ ] Title generator 调 LLM 时只消费 `FragmentScope::TitleGen` 的 Fragment，从协议层加固 `bce0825` 修复
- [ ] Summarizer / compact 路径若读 Bundle，按 `FragmentScope::Summarizer` 过滤
- [ ] Bridge replay（`continuation.rs`）读 Bundle 时按 `FragmentScope::BridgeReplay` 过滤

### 审计总线
- [ ] `ContextAuditBus` trait 定义，含 `emit` / `query(session_id, filter)`
- [ ] 每个 Fragment 进入 Bundle 或被 Bundle 消费时发一条 `ContextAuditEvent`（bundle_id / session_id / at_ms / trigger / fragment / content_hash）
- [ ] `GET /sessions/{id}/context/audit` 返回完整 Fragment 时间线，支持 `since_ms` / `scope` / `slot` 过滤

### 前端 Context Inspector
- [ ] session 右侧抽屉新增 "Context Inspector" tab
- [ ] 时间轴按 Bootstrap → Turn N 分组，每组折叠 Fragment 列表
- [ ] 每条 Fragment 显示 slot / source / scope 徽章 + content 预览（过长时折叠）
- [ ] 顶部支持按 scope / slot 过滤
- [ ] 只读，不提供编辑/禁用按钮（D5 决策）

### 回归
- [ ] 现有 e2e（story owner session、task execution、companion）不回归
- [ ] OwnerBootstrap 轮的 F1 输出与迁移前**逐段等价**（diff 只应来自 scope 过滤产生的可预期差异）
- [ ] title generator 不再吃到 agent 指令相关 Fragment（`bce0825` 场景用测试固化）
- [ ] typecheck / clippy / lint 通过

## Technical Approach

### 数据结构（agentdash-spi）

```rust
// agentdash-spi/src/context_injection.rs

pub enum FragmentScope {
    RuntimeAgent,    // 进入 F1 system prompt
    TitleGen,        // title generator 可见
    Summarizer,      // 压缩器可见
    BridgeReplay,    // 重放历史可见
    Audit,           // 审计总线可见（一般默认包含）
}
pub type FragmentScopeSet = enumset::EnumSet<FragmentScope>;

pub struct ContextFragment {
    pub slot: String,              // 从 &'static str 升级
    pub label: String,
    pub order: i32,
    pub strategy: MergeStrategy,
    pub scope: FragmentScopeSet,   // 新增，默认 RuntimeAgent | Audit
    pub source: String,            // 新增，吸收 HookInjection.source
    pub content: String,
}

pub struct SessionContextBundle {
    pub bundle_id: uuid::Uuid,
    pub session_id: uuid::Uuid,
    pub created_at_ms: u64,
    pub fragments: Vec<ContextFragment>,
}

impl SessionContextBundle {
    pub fn filter_for(&self, scope: FragmentScope) -> impl Iterator<Item = &ContextFragment>;
    pub fn upsert_by_slot(&mut self, fragment: ContextFragment); // slot 唯一，Strategy 控制 merge
    pub fn merge(&mut self, others: impl IntoIterator<Item = ContextFragment>);
    pub fn render_section(&self, scope: FragmentScope, slots: &[&str]) -> String;
}
```

### ExecutionContext 过渡期双字段

```rust
// crates/agentdash-executor/src/connectors/types.rs (或当前 ExecutionContext 所在)
pub struct ExecutionContext {
    pub system_context: Option<String>,                 // deprecated，过渡期并行保留
    pub context_bundle: Option<SessionContextBundle>,   // 新增，优先
    // ... 其余字段不变
}
```

### SessionAssemblyBuilder 的接入点升级

复用项目现有的 [SessionAssemblyBuilder](../../../crates/agentdash-application/src/session/assembler.rs) 作为 context 构建的挂载点，**不引入新的组装中间件**。现有"系统上下文层"字段从 `Option<String>` 升级为 `Option<SessionContextBundle>`：

```rust
// crates/agentdash-application/src/session/assembler.rs
pub struct SessionAssemblyBuilder {
    // ── 系统上下文层 ──（改造）
    system_context: Option<String>,              // deprecated，过渡期保留
    context_bundle: Option<SessionContextBundle>, // 新增，优先
    // ... 其余分层字段不变
}

impl SessionAssemblyBuilder {
    /// 新方法：直接塞入结构化 Bundle（主力入口）
    pub fn with_context_bundle(mut self, bundle: SessionContextBundle) -> Self {
        self.context_bundle = Some(bundle);
        self
    }

    /// 保留方法：过渡期 legacy String 兜底
    #[deprecated(note = "改用 with_context_bundle；过渡期保留")]
    pub fn with_system_context(mut self, markdown: String) -> Self {
        self.system_context = Some(markdown);
        self
    }

    pub fn build(self) -> PreparedSessionInputs {
        // build 时把 bundle 同时写入 PreparedSessionInputs.context_bundle
        // 同时按需渲染 legacy system_context 字符串（过渡期）
        ...
    }
}
```

调用方（4 个 `compose_*`）沿用现有链式 API，只是把原本 `.with_system_context(markdown)` 改为 `.with_context_bundle(build_session_context_bundle(config, contributions))`。**其他 `with_vfs` / `append_lifecycle_mount` / `with_resolved_capabilities` / `append_canvas_mounts` 全部不动**。

### 统一构建器：依赖倒置 + 数据驱动 reducer（D6 核心）

**核心原则**：`context/builder.rs` 模块**不依赖任何 domain 类型**。Builder 退化为纯合并函数，领域自治暴露 `contribute_*() -> Contribution`。

#### 1. context/ 层的小而紧的契约

```rust
// crates/agentdash-application/src/context/builder.rs

pub enum ContextBuildPhase {
    ProjectAgent,
    TaskStart,
    TaskContinue,
    StoryOwner,
    OwnerBootstrap,
    LifecycleNode,
    Companion,
    RepositoryRehydrate,
}

pub struct SessionContextConfig {
    pub session_id: uuid::Uuid,
    pub phase: ContextBuildPhase,
    pub default_scope: FragmentScopeSet,   // 未显式声明 scope 的 Fragment 使用此值
    // 其他全局策略（如 phase-specific slot 过滤规则）
}

pub struct Contribution {
    pub fragments: Vec<ContextFragment>,
    pub mcp_servers: Vec<RuntimeMcpServer>,  // 某些 contribution 自带（MCP / Plugin）
}

impl Contribution {
    pub fn fragments_only(fragments: Vec<ContextFragment>) -> Self {
        Self { fragments, mcp_servers: vec![] }
    }
    pub fn empty() -> Self { Self::fragments_only(vec![]) }
}

/// 纯合并函数。**不认识任何 domain 类型**。
pub fn build_session_context_bundle(
    config: SessionContextConfig,
    contributions: Vec<Contribution>,
) -> SessionContextBundle {
    let mut fragments: Vec<ContextFragment> = contributions
        .into_iter()
        .flat_map(|c| c.fragments)
        .collect();

    apply_default_scope(&mut fragments, config.default_scope);
    merge_by_slot(&mut fragments);    // 替代 filter_user_prompt_injections
    fragments.sort_by_key(|f| f.order);

    SessionContextBundle {
        bundle_id: uuid::Uuid::new_v4(),
        session_id: config.session_id,
        phase: config.phase,
        created_at_ms: now_ms(),
        fragments,
    }
}
```

**Builder 的总代码量预期 < 150 行**（核心合并 + 去重 + 排序 + 默认值填充）。

#### 2. 领域自治：每个模块暴露自己的 `contribute_*`

每个领域模块负责把自己的 domain 对象解包成 Contribution，和 [build_session_plan_fragments](../../../crates/agentdash-application/src/session/plan.rs) 完全同构的纯函数范式。

```rust
// session/plan.rs          ← 已有
pub fn build_session_plan_fragments(input: SessionPlanInput) -> SessionPlanFragments
impl From<SessionPlanFragments> for Contribution { ... }

// task/context.rs
pub fn contribute_task_context(
    task: &Task, story: &Story, project: &Project, workspace: Option<&Workspace>,
) -> Contribution

// story/context_builder.rs
pub fn contribute_story_context(story: &Story, project: &Project) -> Contribution

// project/context_builder.rs
pub fn contribute_project_context(project: &Project) -> Contribution

// session/binding_context.rs (新)
pub fn contribute_agent_binding(binding: &AgentBinding) -> Contribution

// context/source_resolver.rs
pub fn contribute_declared_source(source: &DeclaredSource) -> Contribution

// context/workspace_sources.rs
pub fn contribute_workspace_static_sources(workspace: &Workspace) -> Contribution

// mcp/contribution.rs (新)
pub fn contribute_mcp(config: &McpConfig) -> Contribution
// ↑ 同时带回 RuntimeMcpServer 到 Contribution.mcp_servers

// workflow/context.rs (或复用 context/workflow_bindings.rs)
pub fn contribute_workflow_binding(
    snapshot: &WorkflowSnapshot, bindings: &[WorkflowBinding],
) -> Contribution

// session/instruction.rs
pub fn contribute_instruction(
    task: Option<&Task>, override_prompt: Option<&str>, additional: Option<&str>,
) -> Contribution

// hooks/provider.rs
impl From<&HookSnapshot> for Contribution { ... }
impl From<HookInjection> for ContextFragment { ... }
```

#### 3. 调用方只做聚合（数据驱动的组合）

调用方（`assembler.rs`、story/task/project 的 context_builder）按 phase 自主决定要哪些 contribution，**不再通过 God Object Input 透传**。

```rust
// 示例：story step 的组装
fn compose_story_step(...) -> PromptSessionRequest {
    let config = SessionContextConfig {
        session_id,
        phase: ContextBuildPhase::StoryOwner,
        default_scope: FragmentScope::RuntimeAgent | FragmentScope::Audit,
    };

    let mut contributions = vec![
        build_session_plan_fragments(plan_input).into(),
        contribute_story_context(&story, &project),
    ];
    if let Some(b) = agent_binding {
        contributions.push(contribute_agent_binding(b));
    }
    contributions.extend(declared_sources.iter().map(contribute_declared_source));
    contributions.extend(platform_mcp_configs.iter().map(contribute_mcp));
    if let Some(snap) = workflow_snapshot {
        contributions.push(contribute_workflow_binding(snap, &workflow_bindings));
    }
    contributions.push(contribute_instruction(
        Some(&task), override_prompt, additional_prompt,
    ));
    if let Some(hook_snap) = &hook_snapshot {
        contributions.push(hook_snap.into());
    }

    let bundle = build_session_context_bundle(config, contributions);
    // ...
}
```

#### 4. Hook 注入一视同仁

Hook 链路产出 `HookInjection`，经 `From` 转换为 `ContextFragment`，再包成 `Contribution` 喂给同一个 builder。**agent_loop 每轮重算 bundle 时，Hook Contribution 和其他 contribution 一视同仁参与合并**。

去重由 `merge_by_slot` 完成 —— 如果 `companion_agents` slot 同时被 SessionPlan 和 Hook 产出，后者的 `MergeStrategy` 决定是追加/覆盖/忽略。**`SESSION_BASELINE_INJECTION_SLOTS` 白名单至此完全不需要存在**。

### 设计模式归名

- **依赖倒置**：`context/builder` 不 reach into `task/story/workflow/mcp/...`，反向由各领域模块对 `Contribution` 契约负责
- **Reducer 模式**：builder 是 `reduce(Vec<Contribution>) -> Bundle`，类比 Redux reducer / functional fold
- **数据驱动**：`extra_contributors: Vec<Box<dyn>>` 这种"用对象伪装循环"的反模式清理掉；动态性由 `Vec<Contribution>` 的数据长度表达
- **领域自治**：每个领域决定自己 Domain Object → Contribution 的映射，不被 context/ 层反向侵蚀

### 与项目已有范式的对齐（这不是发明，是回归）

项目内已有三个优秀范式，`ContextContributor` 是唯一的反模式孤岛。本任务把 context 构建拉齐到项目主流：

| 已有好样板 | 位置 | 特征 | 本任务如何对齐 |
|-----------|------|------|---------------|
| 按场景分 Spec | [session/assembler.rs](../../../crates/agentdash-application/src/session/assembler.rs) — `compose_owner_bootstrap(OwnerBootstrapSpec)` / `compose_story_step(StoryStepSpec)` / `compose_lifecycle_node(LifecycleNodeSpec)` / `compose_companion(CompanionSpec)` | 每个场景独立 Spec，字段聚焦、无 Option 膨胀 | 本任务不发明新 Spec；沿用现有 `*Spec` 作为调用方视角的入口，内部组装 `Vec<Contribution>` |
| 声明式 Builder | `SessionAssemblyBuilder` | 分层字段 + `with_/append_` 链式 + `build()`；每步都是小 Contribution | `build_session_context_bundle(Vec<Contribution>)` 是同一理念的数据版本（fold 而非链式），且会被 `SessionAssemblyBuilder::with_system_context` 那层直接吸收 |
| 按触发点分 Input | [agentdash-agent-types/src/decisions.rs](../../../crates/agentdash-agent-types/src/decisions.rs) — `TransformContextInput` / `BeforeToolCallInput` / `AfterToolCallInput` / `AfterTurnInput` / `BeforeStopInput` | 每个 hook trigger 单独 Input struct，字段紧贴触发点 | Hook 产出的 `HookInjection → ContextFragment → Contribution` 转换链沿用这些 Input，不引入新 schema |

### 反模式孤岛：ContextContributor 的历史定位

`ContextContributor` trait 的 doc 写着 "通过 Contributor 模式，新的上下文来源只需实现此 trait 并注册到构建流程，无需修改核心构建逻辑"——这是**为假想的 plugin 扩展做的过度抽象**。

对比项目里其他 `Vec<Box<dyn>>` 使用：

| 使用位置 | 是否真多态 | 判定 |
|---------|-----------|------|
| `Vec<Box<dyn AgentDashPlugin>>` | ✅ 不同 plugin 贡献不同能力集 | 合理 |
| `Vec<Box<dyn VfsDiscoveryProvider>>` | ✅ 不同 provider 探测不同来源 | 合理 |
| `Vec<Box<dyn ExternalServiceClient>>` | ✅ 不同服务不同协议 | 合理 |
| `ToolRegistry<DynAgentTool>` | ✅ 按 name 调度异构 tool | 合理 |
| **`Vec<Box<dyn ContextContributor>>`** | ❌ 6 个 impl 只做"静态产 fragment"或"数据项 × fragment 循环"两种事 | **伪多态** |

真正的 plugin 扩展链路早已走向 `agentdash-plugin-api` crate，`ContextContributor` 成了没被插件接入的孤岛，反而引入了 `ContributorInput: God Object` 和 `extra_contributors: Vec<Box<dyn>>` 的次生坏味道。本任务让 context 构建回归项目主流范式。

### Contributor trait 的退路

`ContextContributor` trait 保留但彻底退居二线：
- `ContextContributorRegistry::with_builtins()` 删除 —— 4 个 builtin（Core/Binding/DeclaredSources/Instruction）迁移为各自领域模块的 `contribute_*` 纯函数
- `Vec<Box<dyn ContextContributor>>` 的 `extra_contributors` 从 [assembler.rs:979](../../../crates/agentdash-application/src/session/assembler.rs#L979) 删除，上层改为组装 `Vec<Contribution>`
- `trait ContextContributor` 保留为 plugin 扩展点；`04-12-plugin-extension-api` 推进时再决定是否收敛为更轻量的 `FnOnce(&PluginCtx) -> Contribution` 函数指针

### ContextComposer 的命运

原 `ContextComposer` 职责全部由新体系吸收：
- **收集 Contributor 产出** → 调用方自己组装 `Vec<Contribution>`
- **排序** → `merge_by_slot` + `sort_by_key(order)`
- **渲染 Markdown String** → 降级为"可选的 legacy 渲染函数 `render_bundle_as_system_context(&Bundle)`"，仅在 `ExecutionContext.system_context: String` 字段未彻底删除前使用
- Composer 模块本身可在后续 cleanup 任务中删除

### Hook 侧改造（D2b 核心）

```rust
// crates/agentdash-application/src/hooks/provider.rs
// 删除：
// - const SESSION_BASELINE_INJECTION_SLOTS: &[&str] = &["companion_agents"];
// - fn filter_user_prompt_injections(...) {...}

// 新增：HookInjection → ContextFragment 转换
impl From<HookInjection> for ContextFragment {
    fn from(injection: HookInjection) -> Self {
        ContextFragment {
            slot: injection.slot,
            label: injection.source.clone(),
            order: default_hook_order(&injection.slot),
            strategy: MergeStrategy::Append,
            scope: FragmentScope::RuntimeAgent | FragmentScope::Audit,
            source: injection.source,
            content: injection.content,
        }
    }
}

// hook_delegate.rs:
// transform_context 内，UserPromptSubmit 后：
//   1. resolution.injections: Vec<HookInjection> → Vec<ContextFragment>
//   2. bundle.merge(fragments)  // upsert_by_slot 自动去重
//   3. build_hook_injection_message(&bundle.filter_for(RuntimeAgent)) → user message
```

**关键变化**：去重从"硬编码白名单"变成"Bundle 层按 slot 隐式去重"。Contributor/SessionPlan 产出的 `companion_agents` slot 和 Hook 产出的 `companion_agents` slot 自动合并（后到者按 `MergeStrategy` 决定覆盖/追加/忽略）。

### F1 重写骨架

```rust
// crates/agentdash-executor/src/connectors/pi_agent/connector.rs
fn build_runtime_system_prompt(
    context: &ExecutionContext,
    tools: &[ToolDescriptor],
) -> String {
    let identity = build_identity_section(context);

    let project_context = match context.context_bundle.as_ref() {
        Some(bundle) => bundle.render_section(
            FragmentScope::RuntimeAgent,
            &["task", "story", "project", "workspace",
              "initial_context", "workflow", "mcp_config", "instruction",
              "declared_source", "static_fragment"],
        ),
        None => context.system_context.clone().unwrap_or_default(),
    };

    let companion = context.context_bundle.as_ref()
        .map(|b| b.render_section(FragmentScope::RuntimeAgent, &["companion_agents"]))
        .unwrap_or_else(|| render_legacy_companion(context));

    let workspace    = render_workspace(&context.vfs);
    let tools_block  = render_tools(tools, &context.mcp_servers);
    let hooks_block  = render_hooks(&context.hook_session);
    let skills_block = render_skills(&context.session_capabilities);

    compose_sections([
        identity, project_context, companion,
        workspace, tools_block, hooks_block, skills_block,
    ])
}
```

### 审计总线

```rust
// crates/agentdash-application/src/context/audit.rs (新模块)

pub enum AuditTrigger {
    SessionBootstrap,
    ComposerRebuild,
    HookInjection { trigger: HookTrigger },
    SessionPlan,
    Capability,
    BundleFilter { scope: FragmentScope },  // 消费侧记录
}

pub struct ContextAuditEvent {
    pub event_id: uuid::Uuid,
    pub bundle_id: uuid::Uuid,
    pub session_id: uuid::Uuid,
    pub at_ms: u64,
    pub trigger: AuditTrigger,
    pub fragment: ContextFragment,
    pub content_hash: u64,   // xxhash(content)，便于跨轮去重观察
}

pub trait ContextAuditBus: Send + Sync {
    fn emit(&self, event: ContextAuditEvent);
    fn query(&self, session_id: uuid::Uuid, filter: AuditFilter) -> Vec<ContextAuditEvent>;
}

pub struct AuditFilter {
    pub since_ms: Option<u64>,
    pub scope: Option<FragmentScope>,
    pub slot: Option<String>,
    pub source_prefix: Option<String>,
}
```

**存储选择**：首版直接用进程内 `Arc<RwLock<VecDeque<...>>>` + 每 session 环形缓冲（比如最近 2000 条）。session_events 持久化等稳定后再加。

**HTTP 暴露**：`crates/agentdash-api/src/routes/acp_sessions.rs` 新增
```
GET /sessions/{id}/context/audit?since_ms=&scope=&slot=&source_prefix=
```
返回 `Vec<ContextAuditEventDto>`，DTO 含 fragment 关键字段 + content 截断到前 2KB。

### 前端 Context Inspector

- 位置：`frontend/src/features/session-context/` 新增 `context-inspector-panel.tsx`，作为右侧抽屉一个 tab
- 轮询 `/sessions/{id}/context/audit`（首版 3s poll；后续可升级 SSE）
- 数据结构：按 `bundle_id` 分组 → 每组按 `trigger` 分小节（Bootstrap / Turn N / Hook / ...） → 每个 Fragment 一行
- Fragment 行：`[scope 徽章] [slot] [source] ... content 预览`
- 顶部过滤器：scope 多选 + slot search 框
- 不提供编辑/禁用（D5 决策）

### 迁移步骤（实施顺序）

1. **SPI 扩 Fragment 字段 + 引入 `SessionContextBundle`**（`agentdash-spi` + 所有 `ContextFragment { ... }` 字面量加 `scope` / `source` 默认值）
2. **在 `context/builder.rs` 引入 `SessionContextConfig` + `Contribution` + `build_session_context_bundle`**（纯函数，不依赖 domain；**此步 context/ 模块的 domain 依赖被反向清理**）
3. **各领域模块暴露 `contribute_*` 纯函数**：
   - task/story/project/workspace 的 core context
   - agent_binding
   - declared_source / workspace_static_source
   - mcp（从 McpContextContributor 搬家）
   - workflow_binding（从 WorkflowContextBindingsContributor 搬家）
   - instruction
4. **`HookSnapshot → Contribution` + `HookInjection → ContextFragment` 转换实现**（保留旧 user message 渲染作为过渡）
5. **ExecutionContext 加 `context_bundle` 字段**（connector 接口升级）
6. **切换 OwnerBootstrap / Story / Task / Lifecycle 入口**：在 `SessionAssemblyBuilder` 新增 `with_context_bundle` 方法；4 个 `compose_*` 聚合 `Vec<Contribution>` → `build_session_context_bundle` → `.with_context_bundle(bundle)`（旧 `.with_system_context(markdown)` 标 deprecated 降级为 legacy fallback）
7. **PiAgent F1 按 Bundle 重写**（Bundle 优先、String fallback）
8. **Hook 侧改造**：`build_hook_injection_message` 输入改 `&[ContextFragment]`；删 `SESSION_BASELINE_INJECTION_SLOTS` / `filter_user_prompt_injections`
9. **删除 `ContextContributorRegistry::with_builtins` + `Vec<Box<dyn>>` 形态的 `extra_contributors`**（trait 本身保留）
10. **审计总线 + HTTP 接口**
11. **前端 Context Inspector 面板**
12. **回归测试 + `bce0825` 场景固化**

每步独立可验证、可部分回滚。步骤 1-4 不影响运行时行为（纯新增 + 领域模块暴露新 API），步骤 6 开始才切换主路径。

**关键验证点**：步骤 2 完成后，`context/` 模块 `cargo check` 应**只依赖 `agentdash-spi` + `uuid` + 标准库**，不再依赖 `agentdash_domain::{task, story, project, workflow, ...}`。这是依赖方向修正的硬性检验。

### 为 D2a 演进留的接口

- `FragmentScopeSet`：D2a 下需要增加 `UserMessage` scope（让 Fragment 直接渲染为 user message），本次**不引入**但类型留好扩展点
- `SessionContextBundle` 拥有 `bundle_id` 和 `created_at_ms`，为未来 `rebuild_dynamic()` 每轮刷新打点铺路
- 审计 `AuditTrigger` 枚举为未来的 `RebuildDynamic` / `TurnEffect` 留事件类型空间

## Related

- 研究地图：[research/context-injection-map.md](research/context-injection-map.md)
- 姊妹任务：[04-29-session-context-builder-d2a-exploration](../04-29-session-context-builder-d2a-exploration/prd.md) — D2a 激进合并讨论存档
- 姊妹任务：[04-29-agents-md-discovery-loading](../04-29-agents-md-discovery-loading/prd.md) — 接 Fragment.source 字段
- 相关提交：`bce0825`（title gen 混入修复）、`30991ef`（bridge 过滤）、`866e42a`（compaction system prompt 修复）、`40f29fd`（挂起会话终止）
