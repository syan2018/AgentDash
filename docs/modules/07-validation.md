# 模块：验证机制（Validation）

## 核心思想

验证是**方式选择**，不是固定流程。

我们不预设"必须人工检查"或"必须用脚本"，因为：
- 代码审查适合Agent，创意设计适合人工
- 标准流程可用脚本，探索性工作需要灵活判断
- 信任度随时间变化（新Agent vs 成熟Agent）

系统只保证：**结果需要验证**，但"如何验证"可自由选择。

## 定位

对Task执行结果和Story完成状态进行验证，确保输出符合预期。

## 职责

- 提供可插拔的验证规则机制
- 支持多种验证方式（Agent审查、脚本检查、人工确认）
- 定义验证通过/失败的标准
- 处理验证失败后的重试或人工介入

## 核心概念

### 验证规则（Validation Rule）
- 定义验证的具体逻辑
- 类型：自动规则（脚本）、半自动规则（Agent审查）、人工规则
- 可配置参数和阈值

### 验证触发点（Validation Trigger）
- Task执行完成后自动触发
- Story完成前触发（聚合验证）
- 手动触发（人工抽检）

### 验证结果（Validation Result）
- 通过（Passed）：符合预期
- 失败（Failed）：不符合预期，需修复
- 警告（Warning）：存在风险，但可接受
- 需要确认（Pending）：需人工判断

## 验证模式

```
自动验证（Automatic）
Task完成 → 自动执行验证脚本 → 返回结果

Agent审查（Agent Review）
Task完成 → 分配验证Agent审查 → Agent返回审查意见

人工确认（Manual Approval）
Task完成 → 发送通知给用户 → 等待用户确认

组合验证（Composite）
Task完成 → 自动验证 → Agent审查 → 人工确认 → 全部通过才算完成
```

## 接口定义（概念层面）

```
Validator {
  registerRule(rule): void
  validate(entityId, ruleIds): ValidationResult
  validateWithAgent(entityId, agentType): ValidationResult
  requestManualApproval(entityId, approverId): ApprovalRequest
}

ValidationRule {
  id: string
  name: string
  type: "script" | "agent" | "manual"
  config: RuleConfig
  criteria: PassCriteria
}

ValidationResult {
  entityId: string
  status: "passed" | "failed" | "warning" | "pending"
  details: ValidationDetail[]
  timestamp: timestamp
}
```

## 关键设计决策（待讨论）

- [ ] 验证规则的定义语言/格式
- [ ] 验证Agent和任务Agent的区分或复用
- [ ] 验证失败后的重试策略
- [ ] 人工确认的时效性和提醒机制

## 暂不定义

- 具体验证脚本语言
- 审查Agent的集成细节
- 验证历史的数据分析
- 验证性能的优化

---

*状态：概念定义阶段*  
*更新：2026-02-21*
