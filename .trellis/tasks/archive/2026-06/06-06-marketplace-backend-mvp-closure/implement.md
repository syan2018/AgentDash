# Implement · Marketplace 后端 MVP 收束

## 执行原则

本任务当前只完成规划，不启动实现。进入实现前需要用户确认并执行 `task.py start`。

实现阶段按 sub-agent 模式派发。每个 subagent 提示必须以当前 active task 路径开头，并要求它直接执行自己的模块，不等待其它 subagent。

## 上下文预读

压缩、换 agent 或开始实现后，先读取：

- `.trellis/tasks/06-05-external-marketplace-sources/prd.md`
- `.trellis/tasks/06-05-external-marketplace-sources/design.md`
- `.trellis/tasks/06-05-external-marketplace-sources/implement.md`
- `.trellis/tasks/06-06-marketplace-backend-mvp-closure/prd.md`
- `.trellis/tasks/06-06-marketplace-backend-mvp-closure/design.md`
- `.trellis/spec/backend/shared-library.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`
- `.trellis/spec/backend/capability/integration-api.md`

代码入口优先读：

- `crates/agentdash-spi/src/platform/marketplace_source.rs`
- `crates/agentdash-contracts/src/shared_library.rs`
- `crates/agentdash-contracts/src/external_marketplace.rs`
- `crates/agentdash-contracts/src/mcp_preset.rs`
- `crates/agentdash-domain/src/shared_library/value_objects.rs`
- `crates/agentdash-domain/src/mcp_preset/value_objects.rs`
- `crates/agentdash-application/src/shared_library/external_marketplace.rs`
- `crates/agentdash-application/src/shared_library/install.rs`
- `crates/agentdash-application/src/mcp_preset/service.rs`
- `crates/agentdash-api/src/routes/marketplace.rs`
- `crates/agentdash-api/src/routes/shared_library.rs`
- `crates/agentdash-api/src/routes/mcp_presets.rs`
- `crates/agentdash-first-party-integrations/src/lib.rs`
- `crates/agentdash-application/src/skill_asset/service.rs`

## 建议子任务派发

### A. MCP Template Domain / Contract

目标：

- 将 `McpServerTemplatePayload` 收束为 `transport_template + route_policy + parameter_schema + capabilities`。
- 新增 `McpTransportTemplate`，MVP 支持 HTTP/SSE URL template。
- 新增 template parameter resolver 所需的领域/应用层类型。
- 扩展 `InstallLibraryAssetRequest` / generated TS contracts，支持 `install_options`.

主要文件：

- `crates/agentdash-domain/src/shared_library/value_objects.rs`
- `crates/agentdash-contracts/src/shared_library.rs`
- `crates/agentdash-contracts/src/generate_ts.rs`
- `packages/app-web/src/generated/shared-library-contracts.ts`

验收：

- payload schema 单测覆盖 valid HTTP/SSE template、缺少 template、非法 schema、未支持 transport。
- contracts 生成产物包含 install options。
- 非 MCP 资产传 MCP install options 会被后续 install 层拒绝。

### B. MCP External Import Validator

目标：

- 在外部 import service 中对 `mcp_server_template` payload 做类型化校验。
- 确认 fetched identity、version、source_ref、payload_digest 语义保持现状。
- 拒绝外部 payload 中的 header/env/credential 值、本机路径、localhost/private network URL。

主要文件：

- `crates/agentdash-application/src/shared_library/external_marketplace.rs`
- `crates/agentdash-spi/src/platform/marketplace_source.rs`
- `crates/agentdash-api/src/routes/marketplace.rs`

验收：

- 外部 MCP import 创建 `remote_imported` LibraryAsset。
- 相同 source_ref 重复 import 幂等更新同一 LibraryAsset。
- source_key/external_id/asset_type mismatch 返回 BadRequest。
- 非法 MCP payload 返回 BadRequest 或 Domain invalid config。
- refresh 对 MCP asset 返回 not_imported / up_to_date / update_available / source_missing。

### C. MCP Install Resolver

目标：

- 扩展 `InstallLibraryAssetInput`，传递 `install_options`。
- 安装 `mcp_server_template` 时用 parameters 解析 `transport_template`，生成具体 `McpTransportConfig`。
- 复用 Project MCP Preset 创建/覆盖逻辑并写入 `InstalledAssetSource`。
- 保持其它 asset type install 行为不变。

主要文件：

- `crates/agentdash-application/src/shared_library/install.rs`
- `crates/agentdash-api/src/routes/shared_library.rs`
- `crates/agentdash-application/src/mcp_preset/service.rs`

验收：

- 无参数 MCP 模板可安装。
- 必需参数齐全时可安装并生成预期 URL。
- 缺少参数、未知参数、参数类型错误、未解析占位符分别失败。
- 安装后的 Project MCP Preset `installed_source.library_asset_id/source_ref/source_version/source_digest` 与 LibraryAsset 一致。
- `overwrite=false` 时 Project key 冲突失败；`overwrite=true` 保留原 id 并更新内容。

### D. Backend Fixture / Gold Path

目标：

- 提供测试 provider 或 first-party fixture source，覆盖 source -> list -> detail -> import -> install。
- 如果使用 first-party fixture，保持 source descriptor 稳定，支持 `mcp_server_template`，可选支持 `skill_template`。

主要文件：

- `crates/agentdash-first-party-integrations/src/lib.rs`
- `crates/agentdash-api/src/routes/marketplace.rs`
- `crates/agentdash-application/src/shared_library/external_marketplace.rs`

验收：

- route/application tests 能在无企业来源时验证 MCP 金线。
- fixture 不绕过 `MarketplaceSourceProvider` registry。

### E. Skill Regression Closure

目标：

- 保留现有 Skill URL Import 到 Shared Library 的行为。
- 保留外部 Marketplace generic import 对 `skill_template` 的支持。
- 检查 Project SkillAsset 外源事实继续由 `InstalledAssetSource` 表达。

主要文件：

- `crates/agentdash-application/src/skill_asset/service.rs`
- `crates/agentdash-api/src/routes/skill_assets.rs`
- `crates/agentdash-application/src/shared_library/external_marketplace.rs`

验收：

- Skill URL Import 测试继续证明 `LibraryAsset(remote_imported)` 先于 Project SkillAsset 写入。
- 外部 fetched `skill_template` payload 仍可导入为 LibraryAsset。
- 冲突保护仍在写入 LibraryAsset 前检查 Project key/source_ref。

### F. Spec And Review

目标：

- 更新 Shared Library / Marketplace Source / cross-layer specs。
- 做一次后端集成 review，确认没有第二套安装状态或运行事实源。

主要文件：

- `.trellis/spec/backend/shared-library.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`
- `.trellis/spec/backend/capability/integration-api.md`

验收：

- spec 说明公共 MCP 模板与安装输入分层原因。
- spec 说明外部 Marketplace refresh 与 Project source-status 的关系。
- spec 说明后续前端只消费 generated contracts 和 source-status。

## 推荐执行顺序

1. A：先定 domain/contract，否则后续 install 与 API 都会返工。
2. B + C：import validator 与 install resolver 可由不同 subagent 并行，但 main agent 需要在合并前统一 payload/option 类型。
3. D：在 B/C 后补金线 provider 或测试 fixture。
4. E：跑 Skill 回归并补缺口。
5. F：规格更新和最终 review。

## 验证命令

实现完成后至少运行：

```powershell
cargo fmt --check
cargo check -p agentdash-contracts -p agentdash-domain -p agentdash-application -p agentdash-api -p agentdash-first-party-integrations
pnpm run contracts:check
cargo test -p agentdash-domain shared_library
cargo test -p agentdash-application shared_library
cargo test -p agentdash-application mcp_preset
cargo test -p agentdash-application skill_asset
cargo test -p agentdash-api marketplace
cargo test -p agentdash-api shared_library
pnpm run migration:guard
git diff --check
```

如果实现改动触及运行期 MCP probe 或 Project MCP Preset route，再补：

```powershell
cargo test -p agentdash-api mcp_presets
cargo test -p agentdash-application mcp_preset::probe
```

本任务不要求前端页面验证，但 contracts 生成必须保持干净。

## 风险与回滚点

- `mcp_server_template` payload 结构变化会影响内置 seed、发布 mapper 和 Marketplace 展示。实现时必须一次性更新 domain、contracts、seed、publish、install 和 tests。
- install request 增加 options 后，所有已有 asset type 必须保持无 options 的默认行为。
- URL template resolver 是安全边界，不能引入任意表达式执行。
- 外部 MCP import 与 Project MCP install 是两个事务边界；import 成功但 install 失败时，LibraryAsset 可以保留，Project 资源不得写入半成品。
- `.trellis/config.yaml` 是本地 dispatch 配置漂移，不属于本任务提交。

## 启动门槛

- 用户确认本规划后，执行 `python ./.trellis/scripts/task.py start 06-06-marketplace-backend-mvp-closure`。
- 启动后再派发 subagent 实现 A/C/B 或按 main agent 当前判断拆分。
