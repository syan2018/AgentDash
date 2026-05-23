# Persistence Ports 分层下沉

## Goal

下沉持久化窄端口与 repository-facing record 类型，降低 `agentdash-infrastructure` 对 `agentdash-application` orchestration 类型的依赖，让 adapter 只实现稳定 persistence contract。

## Requirements

- 盘点 `agentdash-infrastructure` 依赖 `agentdash-application` 的具体类型和原因。
- 优先处理窄而关键的 port：`SessionPersistence`、terminal effect outbox、runtime event persistence、audit persistence、repository-facing DTO。
- 决定 contract 放在 `agentdash-domain`、`agentdash-spi` 还是新 crate，并更新 spec。
- 分批迁移，避免一次性大范围移动导致编译错误难定位。
- 不改变数据库 schema 和对外 API。

## Acceptance Criteria

- [ ] 有依赖图和类型清单，说明哪些 application 类型被 infrastructure 使用。
- [ ] 至少一个关键 persistence port 被下沉到稳定 contract 层，或形成经审阅的分批迁移设计。
- [ ] `agentdash-infrastructure` 对 application orchestration 模块的依赖减少，并有测试/编译验证。
- [ ] `.trellis/spec/backend/architecture.md` 和 `repository-pattern.md` 更新新的依赖边界。
- [ ] 没有引入 API 层对具体 repository 实现的直接编排。

## Out of Scope

- 不强制一次性创建完整 `agentdash-ports` crate。
- 不重写所有 repository。
- 不改变 RepositorySet 的外部使用方式，除非作为分批迁移的一部分。
