# PRD · Marketplace Source SPI 与 Integration Registry

## 背景

父任务 `06-05-external-marketplace-sources` 已确定外部市场来源的首期边界：只覆盖 `skill_template` 与 `mcp_server_template`，外部来源负责发现、分页、详情和拉取候选资产；资产进入平台后必须先落到 Shared Library `LibraryAsset`，再通过 Shared Library install 生成 Project 资源。

本 child 只实现最底层的来源注册能力：轻量 SPI、Host Integration 注册入口、宿主侧 provider registry 和 `source_key` 冲突检测。API、import service、Skill/MCP materializer 和前端体验由后续 child 承接。

## 用户价值

- 企业版源码接入可以声明一个或多个企业 Skill / MCP 市场来源。
- 宿主启动时能统一收集来源并 fail-fast 检测冲突。
- 后续 external marketplace API 可以直接依赖统一 registry，不需要知道每个 integration 的实现细节。

## 确认事实

- `agentdash-integration-api` 是企业仓与开源核心之间的受信契约面。
- `AgentDashIntegration` 已有默认空实现的扩展方法，例如 `library_asset_seeds()`。
- `agentdash-api/src/integrations.rs` 已负责收集 Host Integration，并对 AuthProvider、executor id 等扩展点做冲突检测。
- `agentdash-spi` 已承载轻量平台 SPI，例如 `RemoteSkillSource`、`MountProvider`、`RoutineTriggerProvider`。
- Integration API 规范要求 contract crate 不透出 `tokio`、`axum`、`sqlx`、`reqwest`、`rmcp` 等重运行时依赖。

## 目标

R1. 在轻量 SPI 中定义 Marketplace Source 的 descriptor、query、page、listing、detail、fetched payload、error 与 provider trait。

R2. Provider query/page 必须支持 `cursor`、`limit` 与 `next_cursor`，为外部目录分页和搜索留出稳定合同。

R3. Provider descriptor 必须声明 `source_key`、展示信息、provider kind、trust level、enabled 状态和支持的 `LibraryAssetType`。

R4. 首期 provider 支持类型限定为 `skill_template` 与 `mcp_server_template`；registry 收集阶段校验 source 支持类型。

R5. `AgentDashIntegration` 新增 `marketplace_source_providers()` 默认空实现，并通过 `agentdash-integration-api` re-export 所需 trait/type。

R6. 宿主集成收集结果包含 marketplace source providers，并按 `source_key` 做冲突检测；重复 key 启动失败。

R7. First-party integration 提供一个可测试的空/示例 marketplace source，用于验证企业接入前的合同合理性。

## 非目标

- 不实现 HTTP marketplace routes。
- 不实现 LibraryAsset import / refresh service。
- 不实现 Skill / MCP payload materializer。
- 不实现前端 Marketplace 外部来源 UI。
- 不引入配置式 HTTP catalog、用户级 source 管理或动态 Integration 加载。

## 验收标准

- [ ] `agentdash-spi` 或等价轻量 crate 暴露 Marketplace Source provider trait 与 DTO，且未引入重运行时依赖。
- [ ] `agentdash-integration-api` re-export marketplace source trait/type，并在 `AgentDashIntegration` 上提供默认空注册入口。
- [ ] `collect_integration_registration` 汇总 marketplace source providers。
- [ ] 重复 `source_key` 返回明确 registration error，并有单元测试覆盖。
- [ ] 非首期资产类型被 registry 拒绝或在收集阶段给出明确错误。
- [ ] First-party integration 注册一个测试用空/示例 source，并有测试证明它被收集。
- [ ] 父任务中 API/import/前端 child 可以基于 registry 继续实现，无需回头修改 SPI 的基本分页和 descriptor 合同。

## 开放问题

暂无阻塞问题。实现时可以根据现有 crate 边界选择具体模块路径，但必须保持 contract 轻量。
