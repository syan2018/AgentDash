# 前端重复代码消除 (P1)

## Goal
消除 Workflow/ACP Session UI 中的代码重复，建立共享常量和工具函数。

## Requirements
1. 抽取 COMPLETION_MODE_LABEL / BINDING_KIND_LABEL 等 Workflow 标签常量到共享文件
2. 导出 isRecord 工具函数，消除三处重复定义
3. 删除 AcpTaskEventCard.tsx 中重复的 isTaskEventUpdate 函数，改为导入
4. 统一 PermissionPolicy 类型定义

## Acceptance Criteria
- [ ] Workflow 标签常量只在一处定义
- [ ] isRecord 只在一处定义
- [ ] isTaskEventUpdate 只在一处定义
- [ ] PermissionPolicy 只在一处定义

## Technical Notes
新建共享文件 frontend/src/features/workflow/shared-labels.ts
