# 前端信息暴露清理 (P0)

## Goal
清理前端 Agent/Workflow 相关 UI 中的信息过度暴露问题，确保用户界面只展示面向用户的信息，而非内部实现细节。

## Requirements
1. 重写 Workflow 面板中所有"读起来像开发者注释"的 UI 描述文本
2. 为 WorkflowRun status 和 Phase status 建立中文 label 映射，替代原始枚举字符串直接渲染
3. 隐藏或截断原始 UUID（task ID、toolCallId 等）
4. 将 agent_instructions 从默认展示改为折叠的"开发者详情"区域
5. 将 AcpSystemEventCard 中 Hook 调试级信息（trigger/decision/revision/matched_rule_keys 等）移入折叠调试区

## Acceptance Criteria
- [ ] 所有 Workflow 面板描述文本为用户导向语言
- [ ] 不再有原始枚举值直接渲染到 UI
- [ ] 原始 ID 不在显眼位置展示
- [ ] agent_instructions 默认折叠
- [ ] Hook 调试 chips 默认折叠

## Technical Notes
涉及文件：project-workflow-panel.tsx, task-workflow-panel.tsx, task-drawer.tsx, AcpSystemEventCard.tsx, AcpToolCallCard.tsx, AcpTaskEventCard.tsx
