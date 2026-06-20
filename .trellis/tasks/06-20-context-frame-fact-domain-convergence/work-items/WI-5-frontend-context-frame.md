# WI-5 前端 ContextFrame 展示契约

## Status

completed

## Goal

让前端 ContextFrame UI 与后端事实域协议一致，并对未来 section drift 可诊断。

## Scope

- 对齐后端最终 section 列表。
- CAP snapshot 展示当前能力面，CAP delta 展示变化。
- assignment UI 聚焦任务/workflow/instruction。
- system guidelines UI 聚焦用户偏好和项目规则。
- 未知 section 提供 fallback raw renderer。
- 评估是否继续手写 parser，或引入生成类型以降低 drift。

## Primary Files

- `packages/app-web/src/features/session/model/contextFrame.ts`
- `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx`
- `packages/app-web/src/features/session/ui/ContextFrameStream.tsx`
- `packages/app-web/src/features/session/ui/ContextFrameBody.tsx`
- `packages/app-web/src/features/session/ui/ContextFrameCard.test.tsx`

## Acceptance

- [x] 前端有效 section 覆盖后端协议。
- [x] CAP snapshot/delta 视觉和文案语义清晰。
- [x] 未知 section 不会静默丢失。
- [x] context frame 测试覆盖 capability、assignment、guidelines、unknown section。

## Result

- 前端 frame kind 支持 `capability_state_snapshot` / `capability_state_delta`。
- `ContextFrameSection::ToolSchema` full section 与 `HookInjection` section 的前端分支已移除。
- `unknown_section` fallback 保留原始 section payload，避免后端新增 section 时静默丢失。
- CAP summary 区分 snapshot 与 delta，assignment/guidelines section 继续按各自事实域展示。
