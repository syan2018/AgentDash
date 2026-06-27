# 架构：API contract DTO 边界收敛

## Goal

将 workflow/canvas 等 API routes 从直接返回 domain/application 类型收敛为 agentdash-contracts DTO。

## Requirements

- API route 的 HTTP JSON 响应应以 `agentdash-contracts` 中定义的业务 DTO 为事实源。
- workflow、canvas、workspace module 等 routes 不应直接序列化 domain entity 或 application internal read model。
- Rust contract 与 generated TypeScript DTO 必须保持一致，前端不再依赖 domain/application 的偶然 serde shape。
- 启动实现前需要按 route/resource 拆阶段，明确每阶段的 contract、mapper、前端调用影响。

## Acceptance Criteria

- [ ] 重点 routes 不再直接返回 `AgentProcedure`、`WorkflowGraph`、`LifecycleRun`、application runtime snapshot 等内部类型。
- [ ] API 层包含明确的 contract mapper，且 mapper 不反向污染 application/domain。
- [ ] `pnpm contracts:check` 或等价 contract drift check 通过。
- [ ] 前端 service 使用 generated DTO，不靠手写别名兼容旧字段。

## Notes

- 这是复杂跨层任务，当前只作为 tracking task；不要在补齐设计前 start。
