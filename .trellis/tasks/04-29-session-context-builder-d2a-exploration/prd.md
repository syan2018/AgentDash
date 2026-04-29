# D2a 探索：Hook 与 Bundle 彻底合并的激进方案（讨论存档）

> **状态**：**讨论存档，本任务不执行实施**。当前阶段 D2b 已被选定为实施方案（见姊妹任务 [04-29-session-context-builder-unification](../04-29-session-context-builder-unification/prd.md)）。本文档记录 D2a 激进方案的完整设计思考、代价评估、触发条件，供未来演进时查阅。

## 缘起

在 D2b brainstorm 过程中，讨论了一个更激进的替代方案：**把 Hook 从"每轮产 user message 的 mutator"彻底改造成"每轮产 Fragment 的 producer"，由 Bundle 层统一消费**。

D2b 选择了"统一数据模型、保留执行路径"的折中方案，80% 收益、20% 代价。D2a 是余下 20% 的"概念完美度"，但涉及 agent_loop 核心契约改造，2-3 倍工作量 + 更大风险窗口 + 与当前在途任务冲突。因此本次**主动推迟**。

但 D2a 是 D2b 的**连续演进路径**（不是推倒重来），且 D2b 的 Fragment 数据模型已为 D2a 预留扩展点。值得单独存档。

## D2a 核心动作

### 目标形态

```rust
pub trait FragmentProducer {
    fn lifetime(&self) -> FragmentLifetime;
    fn contribute(&self, ctx: &FragmentCtx) -> ProducerOutput;
}

pub enum FragmentLifetime {
    Static,      // session 级固化（Contributor / SessionPlan / Compact Summary）
    PerTurn,     // 每轮外循环刷新（UserPromptSubmit 产出的 injection）
    SingleUse,   // 消费一次即失效（AfterTurn steering / pending_action）
    Ephemeral,   // 下一轮开始前清掉（BeforeProviderRequest 观测痕迹）
}

pub struct ProducerOutput {
    pub fragments: Vec<ContextFragment>,   // 纯内容注入
    pub effects: Vec<TurnEffect>,          // 副作用（block / transform / abort / ...）
}

pub enum TurnEffect {
    Block { reason: String },
    TransformLastUserMessage { new_text: String },
    DenyTool { call_id: String, reason: String },
    AskApproval { ... },
    AbortTurn { reason: String },
    RefreshSnapshot,
}
```

### agent_loop 外循环改造

```rust
// 当前 D2b：
let ctx = AgentContext { system_prompt, messages, tools };
let new_ctx = hook_delegate.transform_context(ctx).await?;
// mutator 模式，hook 直接改 messages

// D2a 目标：
bundle.rebuild_dynamic(cx);
// ↑ 跑所有 PerTurn Producer，刷新 Fragment
let (next_bundle, effects) = hook_gate.apply_turn_effects(bundle, messages);
// ↑ 纯函数，effects 分开处理
for effect in effects {
    match effect {
        TurnEffect::Block { .. } => break,
        TurnEffect::TransformLastUserMessage { new_text } => { /* mutate */ }
        // ...
    }
}
let system_prompt = render_system_from_bundle(&next_bundle);
let user_msgs     = render_user_messages_from_bundle(&next_bundle);
```

**本质**：agent_loop 从"contex mutator 循环"变成"bundle rebuild + effect 应用"的函数式循环。

## 代价评估

### 代价 1：Hook 返回语义拆分

当前 `HookResolution` 是内容和副作用的大杂烩：

```rust
pub struct HookResolution {
    pub injections: Vec<HookInjection>,        // 内容
    pub transformed_message: Option<String>,   // 副作用：改 user 消息
    pub block_reason: Option<String>,          // 副作用：打断
    pub pending_actions: Vec<HookPendingAction>, // 半内容半副作用
    pub diagnostics: Vec<HookDiagnosticEntry>,
    // ...
}
```

D2a 下必须明确切成两类：
- **内容类** → `ContextFragment`（进 Bundle）
- **副作用类** → `TurnEffect`（进 agent_loop 的 effect 处理器）

**影响面**：
- 所有 preset rule（`hooks/presets/*.rhai`）的返回值协议
- rhai 脚本引擎的 resolution 解析（4 个已知脚本 + 未来任意用户脚本）
- 测试夹具（任何构造 `HookResolution` 的 test helper）

### 代价 2：FragmentLifetime + GC

Bundle 要管 Fragment 生命周期：
- `Static` 永久保留
- `PerTurn` 每轮 rebuild_dynamic 时清理上轮的同 producer 产出
- `SingleUse` 消费即失效（需要 consume ack 机制）
- `Ephemeral` 下轮开始前 GC

涉及：
- Bundle 内部的时间戳 / 序号字段
- Fragment 消费追踪（谁消费过？消费一次还是每次？）
- 审计总线事件语义扩展（Fragment 被消费时也发事件？）

### 代价 3：transform_context 契约颠覆

现在：`transform_context(&mut AgentContext) → Result<()>`（mutator）
D2a：`apply_turn_effects(Bundle, Messages) → (Bundle, Messages, Vec<TurnEffect>)`（pure function）

调用方是 [agent_loop.rs](../../../crates/agentdash-agent/src/agent_loop.rs)（~1800 行），外循环骨架大改。

### 代价 4：Pending Action 归属

当前 Pending Action 是 `hook_session state` 里的动态队列，每轮取一个塞成 user message。

D2a 下两种选择：
- **4a**：Pending Action queue 搬进 Bundle（bundle 需要理解"被消费"的 queue 语义，和 Fragment 的静态性质冲突）
- **4b**：Pending Action → `SingleUse + 带 ack` 的 Fragment，hook_session state 退化成只存"未消费 fragment id"

两种都需要 hook_session state 结构改动。

### 代价 5：Bridge 重放 / Continuation 改造

当前 `continuation.rs build_restored_session_messages_from_events` 从 session_events 重建 `Vec<AgentMessage>`；在 D2a 下：
- Bundle 是权威数据源，重放必须能**从 session_events 重建 Bundle**，而不是 messages
- `convert_messages` 过滤（F4）需要在 Bundle 层表达，或降级为"渲染时跳过"
- `AgentMessage::CompactionSummary` 可以收敛为 Bundle 里的 Static Fragment + messages tombstone

### 代价 6：工作量 + 风险窗口

- 乐观估计 **3 周**，悲观 **1 个月**
- 在新契约稳定前，任何 hook 相关 bug 的 root cause 分析都要区分"旧契约没迁干净" vs "新契约本身写错"，debug 难度翻倍
- 与当前在途任务冲突：`04-21-workflow-agent-replay-on-stop`、`04-21-acp-stream-e2e-workflow-termination`、`04-15-workflow-dynamic-lifecycle-context` 都在动 agent_loop / hook 路径附近

## 激进的好处（如果付得起代价）

### 概念完全统一
- 内容和副作用彻底分离：整个栈只有一种"上下文数据"（Fragment）
- Hook 不再是特殊公民，就是 `FragmentProducer + EffectEmitter`
- 外部插件写 Hook 的心智负担下降

### 浆糊彻底消失
- `SESSION_BASELINE_INJECTION_SLOTS` / `filter_user_prompt_injections` 不存在理由（D2b 也消了，但 D2a 从架构上彻底杜绝同类技术债产生）
- 两条注入链路（Compose / Hook）在代码层面变成**一条**

### Compact 自然收敛
- `AgentMessage::CompactionSummary` 不再是特殊消息类型，而是 `Static Fragment { slot: "compaction/summary", scope: [RuntimeAgent, Summarizer, BridgeReplay] }`
- "替换历史"变成 Bundle 层的 message tombstone
- `session_events` 不再有"某轮 messages 被整段重写"的特殊事件
- `866e42a`（摘要 system prompt 修复）那类跨 bridge 修复不会再复发

### Workflow Lifecycle 动态切换优雅
- Lifecycle step transition 时，只需更新 bundle 的 `PerTurn Producer` 注册表
- 不再绕 `workflow_context_bindings` 那条专用路径
- 与 `04-15-workflow-dynamic-lifecycle-context` 的设计目标高度吻合

### 观测完备
- Bundle rebuild 周期是自然的审计快照点
- 每轮前后的 Fragment 差异直接可见（diff 即是注入变更）
- 场景 1（看到 session 收到了什么）的 UI 时间线天然以 bundle_id 分组

## 触发升级的条件（何时应该重启 D2a）

D2b 落地稳定后，以下任一信号出现可触发 D2a 评估：

**信号 1：Compact 机制需要标准化**
当发现 `AgentMessage::CompactionSummary` 的跨 bridge 特例处理（如 `866e42a` / `30991ef` 这类 fix）频繁出现，说明"消息列表重写"机制无法再优雅扩展。

**信号 2：Workflow Lifecycle 动态步骤需要更灵活的 Fragment 注入**
如果 `04-15-workflow-dynamic-lifecycle-context` 任务推进时发现当前 `workflow_context_bindings` 的静态 binding 不够用，需要运行时根据 step 状态动态注入/撤销 Fragment，D2a 的 `PerTurn Producer` 注册表是自然解。

**信号 3：Hook 插件生态要扩张**
若未来开放用户自定义 Hook（插件市场、rhai 脚本分享等），现有的 "HookResolution 大杂烩" 会让第三方心智负担过高。D2a 的 `Fragment + Effect` 拆分是更干净的插件 API。

**信号 4：D2b 在 PRD 稳定后的"遗留项修复频次"超过预期**
每季度复盘时统计：Hook 副作用相关 fix / compact 相关 fix / `SESSION_BASELINE_INJECTION_SLOTS` 新增需求（如果 D2b 阶段又冒出来）。若频次超过 3-4 次/季度，说明 D2b 的技术债成本已超过 D2a 的一次性投入。

**信号 5：agent_loop 重构任务启动**
如果因为其他原因（性能 / 异步模型变更 / UI streaming 升级）要重写 agent_loop 外循环，顺带做 D2a 的边际成本最低。

## D2b → D2a 迁移路径（事实骨架，非承诺）

D2b 已为 D2a 预留以下接口：

- ✅ `ContextFragment` 字段完备（slot/source/scope/order/strategy）
- ✅ `SessionContextBundle` 存在且是 F1 的主数据源
- ✅ `FragmentScopeSet` 可扩展新 scope（如 `UserMessage`）
- ✅ 审计总线的 `AuditTrigger` 枚举可加新事件（`RebuildDynamic` / `TurnEffect`）
- ✅ `SESSION_BASELINE_INJECTION_SLOTS` 已删除，不再是技术债

D2a 升级时**不需要重做**：Fragment 数据面、Bundle 基础设施、审计总线、前端 Inspector。

D2a 升级**需要做**：
1. 引入 `FragmentLifetime` 枚举 + Bundle GC 逻辑
2. 引入 `FragmentScope::UserMessage`，Fragment 可直接渲染为 user message
3. 拆 `HookResolution` → `Vec<ContextFragment> + Vec<TurnEffect>`
4. 重写 `hook_delegate.transform_context` 为纯函数 `apply_turn_effects`
5. 重写 agent_loop 外循环：bundle rebuild + effect 处理
6. 迁移 Pending Action 到 Fragment 机制
7. 迁移 `AgentMessage::CompactionSummary` 到 Static Fragment + tombstone
8. 重写 `continuation.rs` 从 session_events 重建 Bundle

## Non-Goals（本存档任务不做）

- **不实施任何代码改动**
- **不承诺升级时间点**
- **不设计完整的 rhai 脚本新协议**（升级评估时再细化）
- **不预留 D2a 专用数据字段**（避免过度设计，D2b 够用的字段在升级时按需扩展即可）

## 决策日志

| 日期 | 决策 | 原因 |
|------|------|------|
| 2026-04-29 | D2a 推迟，D2b 先行 | 工作量 3 倍、风险窗口大、与在途任务冲突；D2b 已拿 80% 收益，D2a 延迟不产生新债 |

## Related

- 实施任务：[04-29-session-context-builder-unification](../04-29-session-context-builder-unification/prd.md)
- 研究基础：[../04-29-session-context-builder-unification/research/context-injection-map.md](../04-29-session-context-builder-unification/research/context-injection-map.md)
- 可能触发 D2a 重启的关联任务：
  - [04-15-workflow-dynamic-lifecycle-context](../04-15-workflow-dynamic-lifecycle-context/)
  - [04-21-workflow-agent-replay-on-stop](../04-21-workflow-agent-replay-on-stop/)
  - [04-21-acp-stream-e2e-workflow-termination](../04-21-acp-stream-e2e-workflow-termination/)
