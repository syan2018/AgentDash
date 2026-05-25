# AgentDashboard Spec

`.trellis/spec/` 维护项目的 architecture attractor 及其当前工程投影。它帮助未来 session 判断系统应该向哪里收敛，而不是记录任务过程。

## 内容分层

| 类型 | 含义 | 维护规则 |
| --- | --- | --- |
| Invariants | 长期结构不变量 | 重大思路变化时由人确认，不自动改写 |
| Current Baseline | 当前代码对不变量的工程投影 | 可随实现事实更新，保持简短 |
| Local Decisions | 局部但稳定的设计选择 | 记录为什么，不记录历史流水 |
| Contract Appendices | 协议、DTO、状态流、错误语义 | 可执行契约，避免任务验收材料 |

任务计划、验证命令、closure/audit 证据属于 `.trellis/tasks/`；环境坑和 agent 操作坑属于 `AGENTS.md` 问题收纳。

## 必读顺序

1. [项目总览](./project-overview.md)
2. [技术基线](./tech-stack.md)
3. [沟通规范](./communication.md)
4. 相关 layer architecture：
   - [Backend Architecture](./backend/architecture.md)
   - [Frontend Architecture](./frontend/architecture.md)
   - [Cross-layer Architecture](./cross-layer/architecture.md)

## Layer 索引

- [后端规范](./backend/index.md)
- [前端规范](./frontend/index.md)
- [跨层契约](./cross-layer/index.md)
- [共享规范](./shared/index.md)
- [Thinking Guides](./guides/index.md)

## 阅读规则

- 先读相关模块的 `architecture.md`，理解 role、invariants、current baseline。
- 再读 contract appendices，确认具体字段、状态流和错误语义。
- Guides 只负责提醒“要想什么”，不能替代 architecture 或 contract。
- 不要把历史迁移记录、任务测试清单或完成总结当作长期结构事实。
