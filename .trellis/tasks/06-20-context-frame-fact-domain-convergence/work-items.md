# Work Items

本任务使用单一 Trellis task 跟踪完整 ContextFrame 事实域重构；具体实施切片放在本目录下的工作项文档中管理。每个工作项可以独立完成设计核对、实现、验证和回写状态，但不创建 Trellis child task。

## Index

| ID | Work Item | Status | Document |
| --- | --- | --- | --- |
| WI-1 | ContextFrame 协议与事实域冻结 | planned | [WI-1-context-frame-contract.md](work-items/WI-1-context-frame-contract.md) |
| WI-2 | Capability snapshot/delta 与 companion roster 收束 | planned | [WI-2-capability-domain.md](work-items/WI-2-capability-domain.md) |
| WI-3 | Assignment 与 ProcedureContract 投影收束 | planned | [WI-3-assignment-procedure-domain.md](work-items/WI-3-assignment-procedure-domain.md) |
| WI-4 | Runtime delivery 与 context usage 统计 | planned | [WI-4-delivery-usage.md](work-items/WI-4-delivery-usage.md) |
| WI-5 | 前端 ContextFrame 展示契约 | planned | [WI-5-frontend-context-frame.md](work-items/WI-5-frontend-context-frame.md) |
| WI-6 | Spec、测试与最终集成验收 | planned | [WI-6-spec-validation.md](work-items/WI-6-spec-validation.md) |

## Operating Rules

- 更新状态时先改对应 WI 文档，再同步本索引表。
- 每个 WI 完成时必须记录验证命令和剩余风险。
- 需要拆更细时，在对应 WI 文档内增加 checklist，不创建新的 Trellis task。
- 跨 WI 的设计决策写回 `design.md`，执行顺序写回 `implement.md`。

