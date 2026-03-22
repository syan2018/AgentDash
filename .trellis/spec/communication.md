# 项目沟通规范

> 本项目的所有沟通必须使用中文。

---

## 语言要求

### 强制要求

| 场景 | 语言要求 |
|------|----------|
| 与用户的交流 | 必须使用中文 |
| 文档编写和更新 | 必须使用中文 |
| 代码注释 | 必须使用中文 |
| Git 提交信息 | 必须使用中文 |
| PR 描述 | 必须使用中文 |
| Issue 讨论 | 必须使用中文 |

### 示例

**提交信息（正确）**:
```bash
git commit -m "修复登录页面的样式问题"
git commit -m "feat(auth): 添加用户认证功能"
```

**代码注释（正确）**:
```typescript
// 计算用户的总积分
function calculateTotalPoints(user: User): number {
  // 先过滤掉过期的积分
  const validPoints = user.points.filter(p => !p.isExpired);
  // 然后累加所有有效积分
  return validPoints.reduce((sum, p) => sum + p.value, 0);
}
```

**文档更新（正确）**:
```markdown
## 功能说明

这个功能用于处理用户的订单数据。
```

---

## 为什么使用中文

1. **团队沟通**: 项目团队成员主要使用中文交流
2. **文档一致性**: 所有文档保持统一语言，便于维护
3. **代码可读性**: 中文注释对团队更友好

---

## Git 提交信息规范

### 推荐格式

```bash
git commit -m "type(scope): 中文动作结果"
```

- `type` 使用英文小写，表达提交性质
- `scope` 使用英文小写，表达影响领域，推荐填写
- 冒号后必须使用中文，直接描述本次提交完成了什么
- 描述优先写“动作 + 结果”，避免空泛词，例如“收口一些问题”“调整”“update”

### 推荐 Type

| Type | 适用场景 |
|------|----------|
| `feat` | 新能力、流程打通、用户可感知功能增强 |
| `fix` | 缺陷修复、行为纠偏、回归问题处理 |
| `refactor` | 重构、拆层、抽象调整，但不改变外部语义 |
| `docs` | 文档、规范、任务记录更新 |
| `test` | 测试补齐、测试基线调整 |
| `chore` | 构建、脚本、工程杂项维护 |

### Scope 书写建议

- 优先写领域或模块，而不是笼统写 `misc`
- 推荐示例：`hook`、`workflow`、`frontend`、`executor`、`api`、`task`
- 如果单次提交确实跨多个模块，优先写最主要的领域；不要为了覆盖所有改动写很长的 scope

### 描述书写建议

- 使用中文，聚焦本次提交最关键的结果
- 尽量落到“做成了什么”，而不是“做了一些处理”
- 避免只写文件名、issue 编号或纯技术动作

**推荐示例**:

```bash
git commit -m "feat(hook): 打通 ask 审批与恢复执行链路"
git commit -m "feat(frontend): 完成 hook 事件流与运行态面板联调"
git commit -m "refactor(workflow): 将内置 workflow 收敛为数据驱动 phase 配置"
git commit -m "docs(spec): 明确中文 conventional commit 提交规范"
```

**不推荐示例**:

```bash
git commit -m "update"
git commit -m "修一下问题"
git commit -m "feat: tweak workflow"
git commit -m "feat(frontend): adjust"
```

### 额外要求

- 一个提交只表达一个主要意图，避免把多个无关改动揉成一句模糊描述
- 任务收尾提交优先体现“最终完成的能力”或“收敛后的结构”，不要只写“收尾”“清理”
- 如果提交主要是规范或任务结案，优先使用 `docs(...)` 或 `chore(...)`，不要滥用 `feat`

---

## AI 助手提示

> **重要**: AI 助手在与本项目交互时，必须遵守以下规则：
> - 所有回复必须使用中文
> - 更新文档时必须使用中文
> - 生成的代码注释必须使用中文
> - 建议的提交信息必须使用中文
> - 建议优先使用 `type(scope): 中文动作结果` 这一规范格式
