# 架构：application 边界去耦

## Goal

切断 application 对 executor、relay wire DTO、frontend contracts 的直接依赖。

## Requirements

- `agentdash-application` 不应直接依赖 `agentdash-executor` 实现细节来发现 MCP tools。
- `agentdash-application-ports` 不应在端口签名中泄露 `agentdash_relay` command/response wire DTO。
- `agentdash-application` 不应直接构造 `agentdash-contracts` 的前端 wire DTO。
- application 层应输出应用语义模型或 read model，由 API / local / relay adapter 负责边界映射。
- 启动实现前必须补充 `design.md`，明确 port、adapter、DTO 的归属。
- 启动实现前必须补充 `implement.md`，按可验证阶段拆分，避免一次性大范围改动。

## Acceptance Criteria

- [ ] `agentdash-application` 与 `agentdash-application-ports` 的依赖关系符合 backend spec 的 Interface -> Application -> Domain/SPI 方向。
- [ ] MCP discovery 至少有一个 application-owned port 或 SPI 抽象，executor 只作为实现方。
- [ ] relay wire DTO 只出现在 relay/integration/API adapter 边界。
- [ ] frontend contract DTO 只在 API contract 映射边界构造。
- [ ] 相关 crate 的 `cargo check` 通过，并有跨层边界检查说明。

## Notes

- 这是复杂架构任务，当前只作为 tracking task；不要在补齐设计前 start。
