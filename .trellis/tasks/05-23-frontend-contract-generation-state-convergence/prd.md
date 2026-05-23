# 前后端契约生成与前端状态收拢

## Goal

扩展 Rust -> TypeScript DTO/协议生成，减少前端手写 enum/string normalizer，并将前端 stream hook 与 Zustand store 收拢到更清晰的 feature 边界。

## Requirements

- 盘点当前 generated Backbone protocol 与手写 API DTO/mapper 的覆盖差距。
- 优先选择 Workflow、Session、VFS、Shared Library、MCP Preset、ProjectAgent 等高漂移风险 DTO。
- 增加 generated type drift check 或等价验证脚本。
- 抽象 NDJSON stream transport，减少 Project stream 与 Session stream 的重复实现。
- 将 `useSessionStream` 拆成 transport、normalizer、reducer、React hook 的边界。
- 将 `workflowStore.ts`、`storyStore.ts` 等大 store 中的 API、normalizer、reducer、selector 逐步拆分。

## Acceptance Criteria

- [ ] 有 DTO 生成范围清单和优先级。
- [ ] 至少一组非 Backbone DTO 进入生成链路，或形成经审阅的生成方案。
- [ ] 生成文件有 drift check。
- [ ] 前端手写 normalizer 数量减少，或明确标注仍需手写的原因。
- [ ] session stream 或 project stream 至少一个方向完成 transport/reducer/hook 边界收拢。
- [ ] 前端 type-safety/state-management spec 更新。

## Out of Scope

- 不做 UI redesign。
- 不重写所有 store。
- 不引入与现有构建链冲突的新 schema 工具，除非设计文档明确论证。
