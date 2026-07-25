# 防复发分析

## 根因分类

- **跨层契约缺口**：Native Submit没有明确区分command admission与turn terminal，HTTP、worker和
  interaction各自形成了生命周期。
- **变更传播遗漏**：统一live dispatcher时迁移了feed归约，却漏掉`task_write`对应的Task owner
  invalidation；删除Context Audit API时也没有同步清理producer与UI注册。
- **终态保证不足**：source-owned后台任务缺少error/panic统一terminalization，interaction期间的
  Interrupt也没有直接终结原effect。
- **测试覆盖断层**：已有测试分别证明Native adapter或前端手工状态，但没有保护Accepted、active read、
  Interrupt、usage和owner refresh的关键边界。

## 为什么本次方案能切断循环

1. Dash在同一次原子提交中建立`InputAccepted + TurnStarted + active execution + Accepted effect`，
   随后立即返回receipt；后台执行只推进已存在的owner状态。
2. 所有后台退出路径，包括错误、panic和interaction Interrupt，都会写入原effect的typed terminal；
   `inspect`还能依据Dash终态修复外层Complete effect。
3. provider usage进入native durable history，再由同一canonical projector服务read、changes与live，
   因而实时展示和重连恢复不再依赖两套数据。
4. `task_write`进入统一typed effect planner，executor只重新读取Task owner，不在UI复制Task状态。
5. 旧审计线按producer、bus、API consumer、service、Tab和layout注册整体删除；当前ContextFrame/history
   保持唯一上下文证据。

## 长期保护

- Native Complete集成测试固定覆盖Accepted立即返回、并发幂等、interaction Interrupt、worker panic、
  terminal inspect与多轮usage累计。
- 前端planner/executor测试固定覆盖successful completed `task_write`的精确触发条件。
- 跨层变更评审使用“producer → durable owner → API/live → planner → store → renderer”清单；
  任何入口合并或删除都必须给出完整消费者矩阵。
- 对后台owner task的验收以“每个Accepted effect最终可观测为terminal”为准，而不是以task是否成功spawn为准。
