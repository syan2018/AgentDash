# Hook E2E 回归与脏数据清理

## Goal

为当前 Hook Runtime 建立最低可依赖的回归防线，并清理已识别的 workflow definition / 编码 / session 数据脏状态，避免后续继续被历史脏数据拖累。

## Background

当前我们已经发现并处理过：

- 旧格式 workflow definition 导致反序列化失败
- 需要手动把 workflow run 指向新的 builtin definition
- 部分历史上下文存在中文乱码

同时，新引入的 hook gate 也需要更强的回归验证：

- implement phase 不得直接 completed
- checklist phase 未满足条件时不得 stop
- 前端必须可见 runtime policies / diagnostics

## Scope

- 增加后端单测 / 集成测试 / 前端渲染测试 / E2E 断言
- 清理旧坏 workflow definition
- 记录并处理当前已识别的中文乱码链路
- 形成稳定的 regression checklist

## Requirements

- 不为旧错误格式继续写长期兼容层
- 测试优先覆盖真实用户路径而不是只测辅助函数
- 脏数据清理结果要可复现、可说明

## Acceptance Criteria

- [ ] 后端补 implement/checklist gate 的回归测试
- [ ] 前端补 runtime policy / diagnostics 展示测试
- [ ] 至少 1 条 E2E 验证完整 hook 链
- [ ] 旧坏 workflow definition 有明确清理动作
- [ ] 当前乱码问题形成明确排查/修复记录

## References

- [execution_hooks.rs](crates/agentdash-api/src/execution_hooks.rs)
- [SessionPage.tsx](frontend/src/pages/SessionPage.tsx)
- [trellis_dev_task.json](crates/agentdash-application/src/workflow/builtins/trellis_dev_task.json)
- [AGENTS.md](AGENTS.md)
