# 移除 Project 级会话编排链路

## Goal
移除 Project Settings 中的“会话编排”配置及其跨层数据链路，避免把会话提示词配置错误地挂在 Project 层。

## Requirements
- 删除 ProjectConfig 中的 `session_composition`
- 删除 Project Settings / Project Session 中对应的展示与编辑入口
- 保留 Story 级会话编排能力，但改为独立 `session_composition`，不再以 override 语义存在
- 清理 API / MCP / application / frontend 中相关字段名与消费逻辑

## Acceptance Criteria
- [ ] Project 前后端类型、接口、页面中不再出现 `project.config.session_composition`
- [ ] Story 侧不再使用 `session_composition_override` 命名
- [ ] task/story/project session 上下文构建仍可正常运行
- [ ] 相关检查通过

## Technical Notes
- 项目未上线，不做兼容保留，直接收敛到当前最正确的数据结构
