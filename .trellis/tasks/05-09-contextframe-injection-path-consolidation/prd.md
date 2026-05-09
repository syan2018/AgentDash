# ContextFrame 注入路径收束方案落地记录

## 目标

将 Session 向 Agent 的上下文注入路径收束为统一的 `ContextFrame` 主线，减少重复事实源，明确状态更新语义，确保后续扩展围绕稳定 taxonomy 演进而不再分叉。

## 本期范围（已完成）

本期聚焦“投递机制收束”，不改动既有业务状态机和 connector 主干协议。核心交付如下：

1. 能力状态统一到 `capability_state_update`，并用 `before = null` 表达初始化 delta。
2. `bootstrap_context + workflow_context` 合并为 `mission_context`。
3. 删除 `workspace_surface / skill_surface / hook_runtime_surface / tool_surface(initial)` 废弃入口。
4. `pending_action` 从 bespoke markdown 注入升级为独立 `ContextFrame`。
5. 注入白名单统一为 `MISSION_CONTEXT_SLOTS`，清理 runtime alias 语义漂移。
6. 前端 parser + renderer 对齐新 taxonomy（5 个主类 section）。
7. turn-start notice 的 frame enqueue 入口统一到 `context_frame.rs`。

## 交付结果快照

- 后端新增/重构：
  - `mission_context_frame.rs`
  - `pending_action_context_frame.rs`
  - `hub/runtime_context_transition.rs`
  - `prompt_pipeline.rs`
  - `hook_delegate.rs`
- SPI 变更：
  - `CapabilityState` 新增 `SkillDimension`
  - `CapabilityStateDelta` 新增 `skills`
  - `ContextFrameSection` 收束到新 taxonomy
- 前端对齐：
  - `frontend/src/features/session/model/contextFrame.ts`
  - `frontend/src/features/session/ui/contextFrame/SectionRenderers.tsx`
  - 相关 ContextFrame 测试完成同步更新

## 验证记录

- `cargo fmt --all`
- `cargo check -p agentdash-spi -p agentdash-application`
- `pnpm run frontend:check`
- 关键后端/前端定向测试通过（ContextFrame taxonomy 与 pending action 路径）

## 明确不纳入本期（已确认边界）

以下三项是有意保留的设计边界，不在本次提交内改动：

1. **SessionContextBundle 存废**
   - 结论：`Bundle` 作为 fragment 数据源仍有价值；
   - 本期只收束“投递机制”，不废弃 `Bundle`。
2. **Hook injection -> Bundle turn_delta 审计可见性**
   - 结论：当前是有意的 audit-only 设计；
   - 不影响 LLM 投递主链，不在本期重构。
3. **rendered_system_prompt 存废**
   - 结论：system prompt 通过 `Bundle` 传给 connector 是已定型路径；
   - 本期不做替换。

## 后续评估建议（下一阶段）

1. 为 `turn_delta` 增加“审计索引视图”，降低仅靠日志追踪成本（保持 audit-only 语义不变）。
2. 为 `rendered_system_prompt` 增加一致性断言（Bundle -> connector），防止未来改动时隐性偏移。
3. 为 `SessionContextBundle` 增补“数据源职责边界文档”，避免后续开发误将其当成投递层。

## 关联规格

- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/session/bundle-main-datasource.md`
- `.trellis/spec/backend/hooks/execution-hook-runtime.md`
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`

## 关联任务

- 父任务：`05-09-context-frame-consolidation`
- 同期任务：`05-09-pending-action-context-frame`
