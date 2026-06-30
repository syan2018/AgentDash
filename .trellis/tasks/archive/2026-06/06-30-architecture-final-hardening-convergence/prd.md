# 架构最终彻底收口

## Goal

把上一轮架构 review 收敛后的剩余深层后患一次性制度化处理，避免继续留下“只做第一刀”的处理面。此任务不再拆子任务，集中跟踪四条可并行收束线：

- 质量门 CI adoption：让 `scripts/lib/quality-gates.js` 成为本地与 CI 的共同事实源。
- Generated validator：让生成契约承担运行时校验元数据或校验器，减少手写流形状代码漂移。
- WorkspaceModule pure outcome：让 `WorkspaceModuleAgentSurface` 返回领域化 outcome，由工具适配层负责投影到 `AgentToolResult`。
- AgentRun control-plane direct tests：直接覆盖工作台控制面命令模型，减少对页面 walkthrough 的间接依赖。

## Requirements

- R1：四条收束线放在同一个 Trellis 任务内管理；除非后续明确要求，不再创建子任务。
- R2：每条收束线都必须有明确的完成定义、目标文件面和验证命令，便于并行派发和分 commit 收敛。
- R3：本项目仍处于预研阶段，默认选择正确模型，不引入兼容分支、回退字段或临时双轨 API。
- R4：优先删除、集中或生成浅层处理面；不接受只换位置但继续保留重复解释逻辑。
- R5：涉及跨层契约、生成物、hook/control-plane 约定或质量门约定时，同步更新 `.trellis/spec/` 中可复用的开发规则。
- R6：提交需要按工作流拆清楚；质量门、契约生成、WorkspaceModule outcome、AgentRun 测试应分别提交，最终文档/规格可单独提交。

## Acceptance Criteria

- [ ] 质量门 manifest 被 CI 或根脚本真实采用，不再只是本地辅助清单；相关测试覆盖 gate 命令组合、显示或运行入口。
- [ ] Generated validator 默认按生成层实现：至少覆盖当前 NDJSON envelope 使用的核心生成契约，并让前端流解析消费生成校验能力；若遇到生成层不可落地的硬阻塞，必须记录阻塞原因并实现当前可达到的最强集中化替代方案。
- [ ] `WorkspaceModuleAgentSurface` 的 operation outcome 不再泄漏 `AgentToolResult`；工具适配层拥有最终投影职责，surface 层测试覆盖 invoke 与 present 的主要分支。
- [ ] AgentRun 工作台控制面拥有直接测试，覆盖命令状态、刷新、提交/取消/提升、presentation 等意图映射；页面/ChatView walkthrough 只保留端到端信心，不再承担主要模型覆盖。
- [ ] 所有四条工作线的目标验证命令通过，最终运行一次适合当前改动面的 Trellis check 或等价质量门。
- [ ] 任务完成时补齐实施记录、检查记录、规格更新记录，并归档当前任务。

## Notes

- 这不是“下一刀”任务，而是把已识别的尾部风险一次性关掉的收口任务。
- 可以并行推进，但每条线提交边界必须清楚，避免一个巨型提交把审查和回滚粒度打碎。
