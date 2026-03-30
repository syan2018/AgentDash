# 对比分析：AgentDashboard Hook 引擎 vs Claude Code Hook 系统

> 分析时间：2026-03-30

---

## Claude Code 核心设计

```
事件发生 → JSON stdin → 外部脚本决策 → JSON stdout
```

- 25 种事件类型，统一 API 表面
- 4 种 handler（command / http / prompt / agent）
- 决策只有几种：allow / deny / block / rewrite(updatedInput) / 注入(additionalContext)
- 零内部状态——需要状态的由脚本自己管理
- matcher 就是一个字符串——工具名或事件子类型

## AgentDashboard 核心设计

```
SessionBinding → Owner 解析 → Workflow Projection → Snapshot 构建（多源合并）→ 规则引擎评估 → Resolution
```

- 10 种触发类型（HookTrigger 枚举）
- 静态规则注册表（Rust 编译时 fn 指针三元组）
- 5 种注入形式：context_fragment / constraint / policy / tool 干预 / completion 信号
- 重量级元数据：HookSourceLayer(6层) + HookSourceRef + HookDiagnosticEntry(5字段) + HookPolicyView

---

## 逐项对比

### 1. HookSourceLayer + HookSourceRef 溯源体系 → 过度设计

| | Claude Code | AgentDashboard |
|---|---|---|
| 溯源 | 无。hook 输出就是最终结果 | 每条数据挂 `source_summary: Vec<String>` + `source_refs: Vec<HookSourceRef>`，6 层枚举 |
| 实际使用 | N/A | 只用了 GlobalBuiltin / Workflow / Session 三层，Project/Story/Task 空占位 |

**结论**：简化为 `source: String` 单标签。

### 2. HookPolicyView → 过度设计

| | Claude Code | AgentDashboard |
|---|---|---|
| Policy | `settings.json` 配置本身（permission mode: lax/strict/sandbox） | `policies: Vec<HookPolicyView>`，每条有 key/description/payload/source_summary/source_refs |
| 消费者 | 用户直接看配置 | 规则引擎**从不读取** policy 做决策；只是自然语言重述给前端 |

**结论**：移除。前端从 workflow contract 直接读。

### 3. HookContributionSet 合并框架 → 可简化

| | Claude Code | AgentDashboard |
|---|---|---|
| 合并 | 无。每个 hook 独立返回，顺序执行 | `HookContributionSet` → `merge_hook_contribution()` → `dedupe_source_refs()` + `dedupe_tags()` |
| 贡献源 | 每个 hook 脚本 | 只有 2 个：global_builtin + active_workflow |

**结论**：直接构建 snapshot，去除通用合并抽象。

### 4. context_fragments + constraints 双通道 → 核心区分略冗余

| | Claude Code | AgentDashboard |
|---|---|---|
| 注入通道 | **1 个**：`additionalContext: string` | **3 个**：context_fragments / constraints / policies |
| 软硬区分 | 靠文本措辞 + `decision: "block"` | 独立数据结构 |
| Agent 视角 | 都是上下文文本 | 最终也是拼成 user message 注入 |

**结论**：合并为 `injections: Vec<HookInjection>`，用 `slot` 字段区分用途（如 `slot="constraint"` → BeforeStop gate）。

### 5. HookDiagnosticEntry 诊断体系 → 规模过大

| | Claude Code | AgentDashboard |
|---|---|---|
| 调试 | hook 的 stderr 在 verbose 模式显示 | 每条规则产出 1-3 条结构化 diagnostic（code/summary/detail/source_summary/source_refs），三层累积 |

**结论**：简化为 `{ code, message }` 双字段，或只在 debug 模式产出。

### 6. HookPendingAction 待办系统 → 偏重

| | Claude Code | AgentDashboard |
|---|---|---|
| 待办 | 无。SubagentStop 的 block/allow 是即时决策 | 完整生命周期：5 种 status × 5 种 resolution_kind |
| 场景 | N/A | 只用到 blocking_review 和 follow_up 两种 |

**结论**：简化为 2 种 status（Pending / Resolved）× 2 种 resolution（Adopted / Dismissed）。

### 7. 静态规则注册表 vs 外部脚本 → 各有取舍，保留

| | Claude Code | AgentDashboard |
|---|---|---|
| 规则承载 | 外部进程（任意语言脚本） | Rust 编译时静态 fn 指针 |
| 灵活性 | 极高 | 低（需改代码重编译） |
| 安全性 | 低（任意代码执行） | 高（类型安全） |
| 适用场景 | CLI 开发工具 | SaaS 产品 |

**结论**：保留。静态注册在 SaaS 场景更合适。

---

## 总结：砍/简化/保留

| 组件 | 判定 | 建议 |
|------|------|------|
| `HookSourceLayer` 6 层枚举 | **砍** | → `source: String` |
| `HookSourceRef` 结构体 | **砍** | → `source: String` |
| `HookPolicyView` | **砍** | 前端直接读 workflow contract |
| `HookContributionSet` + `merge_hook_contribution()` | **砍** | 直接构建 snapshot |
| `context_fragments` + `constraints` + `policies` 三通道 | **合并** | → `injections: Vec<HookInjection>` |
| `HookDiagnosticEntry` 5 字段 | **简化** | → `{ code, message }` |
| `HookPendingAction` 5×5 | **简化** | → 2×2 |
| `NormalizedHookRule` 三元组 | **保留** | 核心模型 |
| `hook_rule_registry()` 静态注册 | **保留** | SaaS 合适 |
| 10 种 `HookTrigger` | **保留+扩展** | 新增外部触发类型 |
| `SessionHookSnapshot` | **保留** | 快照模型本身正确 |
| `HookResolution` | **保留** | 决策模型正确 |

**一句话**：规则引擎本身（trigger + matches + apply）设计良好，但围绕它的**元数据包装和观测基础设施**过重——Claude Code 用"一个 string + stderr"就解决了的事情，我们用了 6 个结构体和 3 层溯源。
