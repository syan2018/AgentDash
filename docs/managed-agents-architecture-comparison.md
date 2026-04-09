# Brain / Hands / Session — AgentDashboard 与 Anthropic Managed Agents 架构对比

> **参考文章**：[Scaling Managed Agents](https://www.anthropic.com/engineering/managed-agents) — Anthropic Engineering Blog  
> **分析时间**：2026-04-09

---

## 1. 结论先行

Anthropic 在 Managed Agents 一文中提出将 Agent 系统拆分为 **Brain（大脑）/ Hands（双手）/ Session（会话日志）** 三个虚拟化组件的架构范式。AgentDashboard 从多 Agent 编排的不同出发点，独立收敛到了几乎相同的架构结论。

**这种收敛说明 Brain/Hands/Session 分离很可能是 Agent 系统的基础架构范式，而非偶然的设计巧合。**

### 核心映射表

| Anthropic 概念 | 定义 | AgentDashboard 对应 | 实现位置 |
|---|---|---|---|
| **Brain** | Claude + harness，无状态推理引擎 | `agent_loop()` — 纯函数式内核，state 通过 `&mut AgentContext` 外部传入 | `agentdash-agent/src/agent_loop.rs` |
| **Hands** | 沙箱与工具，通过 `execute(name, input) → string` 标准接口执行 | Local Backend `ToolExecutor` — 无状态工具执行器，通过 relay 协议接收 `command.tool.*` | `agentdash-local/` |
| **Session** | 持久化、append-only 的事件日志，支持崩溃恢复和灵活的上下文访问 | `StateChange` 不可变事件流 + NDJSON streaming + `since_id` cursor resume | `agentdash-domain/` |

---

## 2. Anthropic 文章核心观点提炼

### 2.1 问题陈述

传统 Agent harness 将模型能力假设编码为硬逻辑。当模型升级时，这些假设会过时。文章举例：Claude Sonnet 4.5 需要 "context anxiety" 的 workaround，而 Claude Opus 4.5 完全不需要——硬编码的补偿逻辑反而成了负担。

### 2.2 三组件虚拟化

- **Brain（大脑）**：Claude + harness。现在是无状态的，独立做推理决策，不承担容器开销。
- **Hands（双手）**：沙箱和工具。通过标准接口 `execute(name, input) → string` 执行，可以是容器、手机或其他执行环境。
- **Session（会话）**：持久化的 append-only 事件日志，通过 `getEvents()` 支持崩溃恢复和灵活的上下文查询。

### 2.3 核心设计原则

| 原则 | 含义 |
|---|---|
| **From Pets to Cattle** | 解耦消除单点故障——失败的容器或 harness 是可恢复的，不是灾难性的 |
| **Security Boundaries** | 凭据永远不到达代码执行沙箱；Git token 初始化时注入，OAuth token 存外部 vault |
| **Context Flexibility** | Session 是 Claude context window 之外的可查询上下文对象，支持选择性检索和转换 |

### 2.4 性能收益

解耦后容器只在需要时启动（而非每个 session 都启动），带来：
- p50 TTFT ~60% 改善
- p95 TTFT >90% 改善

---

## 3. 逐维度对照

### 3.1 Brain ↔ AgentLoop

**Anthropic**：Brain 完全无状态，可随时丢弃重建。推理逻辑与容器生命周期解耦。

**AgentDashboard**：`agent_loop()` 的签名体现了相同的无状态设计：

```rust
pub async fn agent_loop(
    prompts: Vec<AgentMessage>,
    context: &mut AgentContext,        // ← 外部状态持有者
    tool_instances: &[DynAgentTool],
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    emit: &AgentEventSink,
    cancel: CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError>
```

关键特征：
- `context` 通过可变引用传入——所有状态变更流经这一个对象
- 循环是纯函数式的：`prompts + context.messages + config → new_messages`
- 无闭包捕获状态；所有回调（`before_tool_call`、`after_tool_call`、`transform_context`）是纯委托点
- 支持 `agent_loop_continue()` 从已有 context 恢复，不需要新 prompt

**对照结论**：两者的 Brain 设计高度一致——都追求**无状态内核 + 外部状态注入**。

### 3.2 Hands ↔ Local ToolExecutor + Relay

**Anthropic**：Hands 通过 `execute(name, input) → string` 标准接口执行。可以是容器、手机或其他环境，互相可替换。

**AgentDashboard**：两种执行模型并存：

| 模式 | Brain 位置 | Hands 位置 | 工具路由 |
|---|---|---|---|
| **第三方 Agent**（Claude Code, Cursor 等） | Local | Local（Agent 自管） | Cloud 发 `command.prompt`，Agent 内部管理工具调用 |
| **PiAgent**（Cloud-native） | Cloud | Local | Cloud 逐个发 `command.tool.*`，Local 的 `ToolExecutor` 无状态执行 |

PiAgent 模式下的工具接口与文章描述几乎同构：

```
Cloud → command.tool.file_read  { workspace_id, path }           → Local
Cloud → command.tool.shell_exec { workspace_id, cwd, command }   → Local
Local → response.tool.*         { result: string }               → Cloud
```

Local 端完全是无状态执行器——不知道 Brain 的意图，只做 RPC 执行。

**对照结论**：PiAgent 的 `command.tool.*` 协议是 `execute(name, input) → string` 的直接实现。第三方 Agent 模式则将 Brain+Hands 打包在 Local 运行，是一种务实的扩展。

### 3.3 Session ↔ StateChange Event Log

**Anthropic**：Session 是持久化的 append-only 事件日志，通过 `getEvents()` 提供崩溃恢复和上下文查询。

**AgentDashboard**：`StateChange` 实体实现了完全相同的语义：

```rust
pub struct StateChange {
    pub id: i64,                    // 单调递增 ID，用于 cursor-based resume
    pub project_id: Uuid,
    pub entity_id: Uuid,
    pub kind: ChangeKind,           // 严格枚举：StoryCreated, TaskStatusChanged...
    pub payload: serde_json::Value, // Delta 数据
    pub created_at: DateTime<Utc>,
}
```

特征对照：

| 属性 | Anthropic Session | AgentDashboard StateChange |
|---|---|---|
| 持久化 | 是 | 是（SQLite） |
| Append-only | 是 | 是（不可变，只增不改） |
| 崩溃恢复 | `getEvents()` | `since_id` cursor + NDJSON resume |
| 上下文查询 | 选择性检索 + 转换 | Frontend 通过 `x-stream-since-id` 增量获取 |

**对照结论**：完全一致。两者都将 Session 视为**真相之源（source of truth）**，而非临时缓存。

### 3.4 Context 管理对比

**Anthropic**：Session 作为 context window 之外的可查询对象，支持选择性检索，避免不可逆的数据丢失。

**AgentDashboard**：**这是项目最强的差异化领域**——不仅有 event log 查询，还构建了完整的三层 context pipeline：

| 层 | 职责 | 对应 Anthropic |
|---|---|---|
| **Address Space**（统一 VFS） | 通过 mount 抽象统一 workspace/spec/brief/km 等来源 | 无直接对应——文章仅提 `getEvents()` |
| **Context Injection** | 声明式定义注入什么（File / ProjectSnapshot / StoryContext / WorkflowArtifacts） | Session 的选择性检索 |
| **Hook `transform_context`** | 运行时动态改写上下文 | 无直接对应 |

AgentDashboard 的 context 模型比文章描述的更精细。文章中 Session 既是存储又是查询入口；AgentDashboard 将"存什么"和"怎么取"拆成了独立的关注点。

### 3.5 安全边界对比

**Anthropic**：凭据永远不到达代码执行沙箱。Git token 在初始化时注入；OAuth token 存在外部 vault，通过代理访问。

**AgentDashboard**：安全模型侧重**数据归属隔离**而非凭据隔离：

| 规则 | 说明 |
|---|---|
| Cloud 不直接访问 Local 文件 | 所有文件操作通过 relay 路由 |
| Local 不写业务数据库 | 执行状态与业务状态严格分离 |
| Path Safety Contract | `accessible_roots` 白名单 + `..` 路径逃逸拒绝 |

**差距**：当前模型假设 Local Backend 是受信任的。随着未来引入更多外部集成（Git push、API 调用、第三方服务），需要考虑文章中的 **credential vault + proxy** 模式，避免凭据泄漏到执行沙箱。

---

## 4. AgentDashboard 的差异化优势

Anthropic 的文章聚焦于**单 Agent 的规模化部署**；AgentDashboard 的出发点是**多 Agent 的协同编排**。这带来了若干文章未涉及的能力：

### 4.1 多 Agent 编排

- 支持异构 Agent（Claude Code / Cursor / Codex / PiAgent）在统一框架下协作
- Task 绑定 `AgentBinding`（执行器类型 + 配置），而非绑定具体 Agent 实例
- Story → Task 分解支持不同 Task 分配给不同类型的 Agent

### 4.2 双执行模型

文章只有一种模型（Cloud Brain + Remote Hands）。AgentDashboard 支持两种：
- **Cloud-native**（PiAgent）：Brain 在 Cloud，直接访问业务数据，tool call 路由到 Local
- **Third-party**（Claude Code 等）：Brain+Hands 都在 Local，Cloud 只做 prompt 下发和状态收集

这种灵活性让系统既能管理不可控的外部 Agent，又能运行完全自控的内部 Agent。

### 4.3 Hook 治理体系

文章未涉及执行层面的治理。AgentDashboard 通过 Hook Runtime 实现了完整的执行治理：
- `before_tool_call` → 工具调用门控（Allow / Deny / Rewrite / Ask）
- `after_turn` → 每轮后的方向校正
- `before_stop` → 完成前的最终验证
- Hook 来源分层（global → workflow → story → task → session），支持策略叠加

### 4.4 Companion/Subagent Dispatch

主 Agent 可以派发子任务给 Companion Agent，通过 `before_subagent_dispatch` hook 控制是否允许，结果通过 `pending_actions` 回传。这是文章中"多 Brain 独立运行"理念的具体实现。

---

## 5. 值得借鉴的观点与改进方向

### 5.1 Brain 可丢弃性 → PiAgent 崩溃自动恢复

**文章观点**：Brain 完全无状态，可随时丢弃重建。"From Pets to Cattle"。

**现状**：`agent_loop()` 的签名已经是无状态的，但 PiAgent 运行在 Cloud 进程中，如果该进程崩溃，`AgentContext`（包含已积累的 message history）的恢复路径尚不明确。

**改进方向**：
1. 确保 `AgentContext` 可以完全从 session event log 重建——每次 LLM 返回后将关键状态写入 event log
2. 实现 PiAgent Brain 崩溃后的自动 spawn + replay：新实例从 event log 恢复到崩溃点继续
3. 考虑 Brain Pool——多个 Brain 实例消费同一任务队列，任意实例故障不影响整体

### 5.2 模型能力会变 → Model Capability Profile

**文章观点**：harness 中编码的模型能力假设会过时。Sonnet 4.5 需要的 workaround 在 Opus 4.5 上完全不需要。

**现状**：AgentDashboard 的 pluggable strategy 哲学已为此留了空间，但尚未显式建模。

**改进方向**：
- 将 **Model Capability Profile** 作为 `AgentBinding` 的一部分：
  - 上下文窗口敏感度 → 影响 compaction 策略
  - Tool calling 可靠性 → 影响 retry 策略
  - 自主性水平 → 影响 checkpoint reminder 频率
- 模型升级时只需更新 profile 配置，无需改动 harness 代码
- Hook policy 可基于 profile 字段做条件分支（如：对低自主性模型启用更频繁的 `after_turn` 检查）

### 5.3 凭据隔离模型 → Credential Vault + Proxy

**文章观点**：凭据永远不到达执行沙箱。OAuth token 存在外部 vault，通过代理访问。

**改进方向**：
- 当前 Local Backend 可直接使用本机凭据（Git SSH key、环境变量中的 token 等），这在可信环境下合理
- 未来如果支持远程/多租户执行环境，应引入：
  - Credential Vault（集中存储凭据）
  - Proxy Layer（执行沙箱通过代理访问外部服务，代理注入凭据）
  - 凭据生命周期管理（自动轮换、按 session 作用域分发）

### 5.4 懒初始化 → 解耦容器供给与 Session 创建

**文章观点**：解耦后容器只在需要时启动，p50 TTFT ~60% 改善，p95 TTFT >90% 改善。

**改进方向**：
- 当前 Local Backend 需要持续 WebSocket 连接，但隔离环境（Git worktree 等）可以做懒初始化
- 只有当 `command.tool.*` 到达时才 spin up 对应 workspace 的隔离环境
- Session 创建时只分配 metadata，不预热执行资源
- 这在多 workspace 场景下尤其重要——避免为每个潜在的 workspace 都维持 ready 状态

### 5.5 多 Brain 协作 → 共享 Event Log 而非直接通信

**文章观点**："Multiple brains can operate independently; hands remain interchangeable."

**改进方向**：
- 当前多 Agent 协作以 Task 分片为主（各干各的），Companion 机制提供了初步的 Agent 间协调
- 可以进一步探索：多个 Brain 通过共享 Session event log 感知彼此的进展，而非直接通信
- Event log 既是恢复机制，也是协作机制——一个 Brain 写入的事件可以被另一个 Brain 读取作为上下文
- 这避免了复杂的 Agent 间通信协议，保持了架构的简洁性

---

## 6. 架构启示

### 6.1 Brain/Hands/Session 是基础范式

两个项目从不同出发点——Anthropic 从单 Agent 规模化，AgentDashboard 从多 Agent 编排——独立收敛到相同的三组件分离架构。这种收敛强烈暗示：

> **Brain/Hands/Session 分离是构建可扩展 Agent 系统的基础架构范式，正如 MVC 之于 Web 应用、Actor Model 之于并发系统。**

### 6.2 关键共识

1. **Brain 必须无状态**——所有状态外部化到 Session，Brain 实例可丢弃可替换
2. **Hands 必须标准化**——`execute(name, input) → result` 是最小公约接口，Hands 不需要理解 Brain 的意图
3. **Session 必须持久化**——append-only event log 同时解决崩溃恢复、上下文查询、审计追踪三个问题
4. **解耦带来的性能收益是真实的**——不是为了架构美感，而是有可量化的 TTFT 改善

### 6.3 后续演进建议

| 优先级 | 方向 | 依据 |
|---|---|---|
| P1 | PiAgent Brain 崩溃自动恢复 | 文章核心论点；当前架构已就绪，差 replay 实现 |
| P2 | Model Capability Profile | 文章开篇教训；AgentDashboard 支持多模型，更需要此机制 |
| P3 | 工具执行懒初始化 | 有量化性能数据支撑；多 workspace 场景收益明显 |
| P4 | Credential Vault | 当前可信环境下非必须；远程/多租户时必须 |
| P5 | Event Log 作为 Brain 间协作介质 | 探索性方向，需验证可行性 |

---

*更新：2026-04-09*
