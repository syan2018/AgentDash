# Work Item 05: Frontend And Contract Legacy Cleanup

## Status

Planned.

## Goal

清理前端和生成 contract 中仍暴露旧 RuntimeSession 入口、旧 flat skills fallback、deprecated re-export 或 deprecated backbone event 的路径。

## Scope

- 删除 `SessionChatViewTypes` 中 RuntimeSession 创建/切换旧 props。
- 删除 `workspaceRouting.ts` deprecated re-export，并把调用方改为直接使用 `workspaceTerms.ts`。
- 收紧 `SessionCapabilityCard` / `ContextOverviewTab` 的 flat skills fallback，确认 `skill_clusters` 是唯一展示 contract 后删除相关旧 fixture。
- 评估 generated backbone protocol 中 deprecated event 的 Rust 源头；若 server 不再 emit，则删除 Rust enum、generated 类型和前端 reducer 分支。

## Guardrails

- Canvas 不从旧 `uri` fallback 打开的测试属于拒绝旧路径的守卫，保留或用更直接的当前 contract 测试替代。
- stream transport 不读取旧 `id` fallback 的测试属于事件序列契约守卫，保留。

## Affected Areas

- `packages/app-web/src/features/session/ui/SessionChatViewTypes.ts`
- `packages/app-web/src/features/workspace/model/workspaceRouting.ts`
- `packages/app-web/src/features/session/ui/SessionCapabilityCard.test.tsx`
- `packages/app-web/src/features/session/model/sessionCapabilitiesBlock.ts`
- `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx`
- `packages/app-web/src/generated/backbone-protocol.ts`
- Rust backbone protocol source for generated frontend contracts.

## Dependencies

与 Work Item 04 共享 contract 清理判断；若删除 generated 类型，必须从 Rust 源头和生成流程同步处理。

## Validation

- 对应前端单测切片。
- contracts 生成/检查命令。
- `pnpm --filter app-web test -- --runInBand` 或项目当前等价测试切片。
