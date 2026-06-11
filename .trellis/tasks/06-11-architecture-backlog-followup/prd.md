# 架构 backlog 后续评估

## 背景

`review-refactor-quality-sweep` 已经完成一轮模块级快速清理，并把超出单模块快修范围的设计问题收敛到严格架构 backlog。那些条目通常涉及跨模块事实源、公共 contract、运行时控制面或前后端共同消费的协议，需要独立设计、拆分和验收。

本任务承接这些架构项，作为后续具体评估和拆分入口。当前任务保持 planning 状态，先组织问题、优先级和产出标准；进入实现前应按具体 ARCH 项补充 `design.md` / `implement.md` 或拆成子任务。

## 目标

- 保留 `review-refactor-quality-sweep` 形成的 ARCH 编号、证据和优先级，避免架构问题散落在快修任务归档后失去入口。
- 按影响面和依赖关系评估每个 ARCH 项，明确它应该设计、拆分、合并还是降级为模块级重构。
- 为 P1 项优先形成可执行设计结论，包含事实源归属、边界变化、数据流、验证方式和迁移顺序。
- 把可以独立实施的架构项拆成后续 Trellis 子任务或任务组，让当前快修任务可以干净归档。

## 范围

来源：`.trellis/tasks/06-11-review-refactor-quality-sweep/architecture-backlog.md`

当前待评估条目：

| ARCH | 优先级 | 标题 | 主要影响面 |
| --- | --- | --- | --- |
| ARCH-001 | P1 | inline mutation 存在 API 与 Agent runtime 两套语义 | VFS API mutation、Agent runtime overlay、inline_fs 持久化、冲突处理 |
| ARCH-002 | P1 | workflow ready node 启动链路有两套入口 | workflow dispatch、lifecycle start、orchestration scheduler、NodeStarted 事实源 |
| ARCH-003 | P1 | 生命周期状态事实源分散 | lifecycle status projector、scheduler、view、active run selection |
| ARCH-004 | P1 | session running/control 状态事实源分散 | runtime-control、execution projection、chat UI running/action 状态 |
| ARCH-005 | P1 | Session UI 直接消费完整 BackboneEvent | session feed view model、tool card registry、generated event contract |
| ARCH-006 | P2 | runtime tool composer 完整迁出 VFS | VFS tool factory、session runtime composer、API bootstrap、runtime ready gate |
| ARCH-007 | P2 | local-runtime CommandHandler 服务边界过宽 | local runtime command bus、prompt/tool/MCP/terminal/extension command services |
| ARCH-008 | P2 | prompt MCP relay contract 仍是 raw JSON | relay protocol、backend transport port、application/local prompt parser |
| ARCH-009 | P2 | Extension Host process/env sandbox contract 未定义 | extension permission contract、process/env policy、SDK 文档与样例 |
| ARCH-010 | P1 | companion platform broker 与权限授权闭环未统一 | PermissionGrant、AgentFrame capability、审批 UI/API、runtime projection |
| ARCH-011 | P2 | Canvas CRUD DTO 事实源未进入 contracts | agentdash-contracts、API canvas DTO、generated TS、前端 canvas service/types |
| ARCH-012 | P2 | Workflow auto-granted baseline 跨层事实源重复 | backend visibility rules、generated contracts、workflow capability panel |

## 处理原则

- 每个 ARCH 项先做设计评估，再决定是否进入实现；设计评估应引用真实代码证据和相关 `.trellis/spec/` 契约。
- P1 项优先，尤其是 workflow/session/lifecycle/companion 事实源问题。
- 涉及共享 contract、数据库、生成类型或跨端协议时，设计必须包含验证命令和迁移顺序。
- 能独立实施的内容拆为子任务；仍需讨论的设计问题保留开放状态和待确认问题。

## 验收标准

- [ ] 每个 ARCH 项都有明确状态：待设计、已拆子任务、已合并到其它 ARCH、已降级为模块级重构、已关闭。
- [ ] 每个 P1 ARCH 项至少有一份设计评估，覆盖事实源归属、影响文件/模块、建议边界、风险和验证方式。
- [ ] 已决定实施的 ARCH 项创建对应 Trellis 子任务或独立任务，并写清验收标准。
- [ ] 已降级或合并的 ARCH 项在本任务中记录原因和目标归属。
- [ ] 快修任务 `06-11-review-refactor-quality-sweep` 归档后，本任务仍能作为架构后续入口独立使用。
