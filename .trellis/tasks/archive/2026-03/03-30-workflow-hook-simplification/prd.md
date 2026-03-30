# Workflow Hook 引擎简化重构

## 背景

当前 Hook 引擎围绕 `NormalizedHookRule`（trigger + matches + apply）的核心模型是好的，
但**围绕核心的元数据包装、溯源体系、展示基础设施**过于繁重：

- 6 层 `HookSourceLayer` 枚举只用了 3 层，每条数据都挂满 `source_summary + source_refs`
- `HookPolicyView` 是规则的自然语言重述，规则引擎本身从不读取它做决策
- `HookContributionSet` 通用合并框架只服务于 2 个贡献源（global_builtin + workflow）
- `context_fragments` vs `constraints` 双通道在 Agent 视角最终都是注入文本
- `HookDiagnosticEntry` 结构化诊断每次评估产出 5-10 条，规模过大
- `HookPendingAction` 有 5 种 status × 5 种 resolution_kind，实际只用到 2 种场景

对标 Claude Code 的 hook 设计：**事件 → JSON stdin → 脚本决策 → JSON stdout**，
机制极简但通过外部脚本实现任意复杂度。

## 目标

1. **精简元数据包装**：去除无实际消费者的溯源/展示层
2. **统一注入通道**：将 fragment/constraint/policy 合并为更少的概念
3. **保留并增强核心能力**：trigger + matches + apply 规则模型不动
4. **支持外部触发入口**：不仅限于 Agent 生命周期事件，还支持：
   - 其它 Agent/Session 的消息回流
   - 业务状态变更注入（如 Task 状态转移）
   - 内置条件触发注入（如定时、阈值）
   - 这些入口也必须是**规则化且精简的**，和 Agent 生命周期事件共享同一套规则引擎

## 设计原则

### P1：单一注入通道
- 合并 `context_fragments` + `constraints` + `policies` 为 **`HookInjection`**
- `HookInjection` 有 `slot`（用途标识）、`content`（注入文本）、`source`（单字符串标签）
- 硬约束通过 `slot = "constraint"` 区分，不再是独立数据结构
- 精简后 delegate 层根据 `slot` 做差异化处理（如 constraint slot → BeforeStop gate）

### P2：诊断降级
- `HookDiagnosticEntry` 简化为 `HookDiagnostic { code: String, message: String }`
- 去除 `source_summary`、`source_refs`、`detail` 字段
- 只在 debug/verbose 模式下记录完整诊断，正常模式只记录 matched_rule_keys

### P3：溯源极简化
- 去除 `HookSourceLayer` 枚举和 `HookSourceRef` 结构体
- 用 `source: String` 单标签替代（如 `"builtin:workspace_path_safety"`、`"workflow:trellis_dev_task:implement"`）
- Snapshot 级保留 `sources: Vec<String>` 用于审计

### P4：去除 PolicyView
- 删除 `HookPolicyView` 及相关构建逻辑
- 前端如需展示策略信息，从 workflow contract 直接读取，不经过 Hook 层转述

### P5：去除 ContributionSet 合并框架
- 删除 `HookContributionSet` 和 `merge_hook_contribution()`
- `load_session_snapshot()` 直接构建 snapshot，不经过中间抽象

### P6：PendingAction 简化
- `HookPendingActionStatus` 简化为 `Pending | Resolved`（Injected 合并到 Pending）
- `HookPendingActionResolutionKind` 简化为 `Adopted | Dismissed`（其余合并）

### P7：外部触发入口（新增能力）

扩展 `HookTrigger` 枚举，增加非 Agent 生命周期的触发源：

```
// 现有（Agent 生命周期）
SessionStart, UserPromptSubmit, BeforeTool, AfterTool,
AfterTurn, BeforeStop, SessionTerminal,
BeforeSubagentDispatch, AfterSubagentDispatch, SubagentResult

// 新增（外部触发）
ExternalMessage,      // 其它 Agent/Session 的消息回流
StateChange,          // 业务实体状态变更（Task/Story 状态转移、WorkspaceBinding 变化等）
ConditionMet,         // 内置条件触发（如计时器、检查通过阈值、artifact 数量达标等）
```

外部触发与 Agent 生命周期触发**共享同一套规则引擎**（NormalizedHookRule），
区别仅在于 trigger 匹配和 query.payload 的结构。

触发入口统一通过 `HookSessionRuntimeAccess.evaluate()` 调用，
外部系统只需构造正确的 `HookEvaluationQuery` 即可，无需知道规则引擎内部实现。

## 不动的部分

| 组件 | 原因 |
|------|------|
| `NormalizedHookRule` 三元组 | 核心模型，简洁有效 |
| `hook_rule_registry()` 静态注册表 | 类型安全，SaaS 场景合适 |
| `HookTrigger` 枚举（现有的 10 种） | 覆盖完整，不需要砍 |
| `SessionHookSnapshot` 作为状态视图 | 快照模型本身是对的 |
| `HookResolution` 作为决策输出 | 决策模型本身是对的 |
| `HookSessionRuntime` + `HookRuntimeDelegate` | 运行时层不受影响 |
| `ActiveWorkflowProjection` 读模型 | workflow 投影逻辑不变 |
| Completion 评估逻辑 | 步进机制不变 |

## 验收标准

- [ ] `HookSourceLayer` 枚举和 `HookSourceRef` 结构体被移除，用 `source: String` 替代
- [ ] `HookPolicyView` 被移除，snapshot 不再携带 policies 字段
- [ ] `HookContributionSet` 和 `merge_hook_contribution()` 被移除
- [ ] `context_fragments` + `constraints` + `policies` 合并为 `injections: Vec<HookInjection>`
- [ ] `HookDiagnosticEntry` 简化为双字段
- [ ] `HookPendingAction` status 简化为 2 种，resolution_kind 简化为 2 种
- [ ] 新增 `ExternalMessage` / `StateChange` / `ConditionMet` 三种外部触发类型
- [ ] 新增触发类型至少有 1 条注册规则和对应单元测试
- [ ] 所有现有单元测试迁移通过
- [ ] `cargo check` 全 crate 通过

## 风险与注意事项

- **前端适配**：snapshot 结构变化会影响前端 workflowStore / hook 展示组件，需同步更新
- **API DTO 适配**：`agentdash-api` 层的 workflow DTO 需同步精简
- **渐进式迁移**：可考虑先做 P1-P5（精简），再做 P7（新增外部触发），分 2 个 PR
- **Completion 逻辑**：completion 评估读取 snapshot 的方式需要适配新的 injection slot 模式

## 参考文档

| 文档 | 说明 |
|------|------|
| [ref-claude-code-hooks.md](./ref-claude-code-hooks.md) | Claude Code 官方 hook 系统完整参考（25 种事件类型、4 种 handler、决策控制汇总） |
| [ref-comparison-analysis.md](./ref-comparison-analysis.md) | AgentDashboard vs Claude Code 逐项对比分析（砍/简化/保留决策依据） |

## 技术备注

参考 Claude Code hook 设计的核心洞察：
- **机制简单，脚本复杂**——引擎只做路由和决策收集，复杂度推到消费侧
- **单一注入通道**——所有上下文注入都走 `additionalContext: string`
- **无溯源负担**——hook 脚本的输出就是最终结果
- **外部进程模型**——我们不需要照搬（我们是 SaaS），但要学它的"引擎不承担解释责任"哲学
