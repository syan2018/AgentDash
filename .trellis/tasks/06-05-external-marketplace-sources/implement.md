# Implement · 外部市场来源接入规划

## 执行策略

本任务是 parent planning task，不直接进入实现。实现期建议创建 child tasks，每个 child 可独立规划、实现、检查和归档。parent 负责维护源需求、边界、跨 child 验收和最终集成 review。

## Child 拆分

### 1. Marketplace Source SPI 与 Integration Registry

目标：

- 在轻量 SPI / Integration contract 中定义 `MarketplaceSourceProvider`、descriptor、query、listing、error。
- `AgentDashIntegration` 暴露 source provider 注册入口。
- API composition 收集 providers，执行 `source_key` 冲突检测。
- 来源治理收敛到源码级 Host Integration，用户级来源管理和配置式 catalog 另按独立治理模型推进。

主要文件：

- `crates/agentdash-spi/src/platform/...`
- `crates/agentdash-integration-api/src/...`
- `crates/agentdash-api/src/integrations.rs`
- `.trellis/spec/backend/capability/integration-api.md`

验收：

- 重复 `source_key` 启动失败。
- first-party integration 提供一个可测试的空/示例企业分发 source。
- contract crate 不引入重运行时依赖。

### 2. Enterprise Marketplace API 与 Contracts

目标：

- 新增 source/listing/detail/import API。
- DTO 进入 `agentdash-contracts` 并生成前端 TS contract。
- import service 将 fetched asset 转成 typed `LibraryAssetPayload`。

主要文件：

- `crates/agentdash-contracts/src/...`
- `crates/agentdash-api/src/routes/...`
- `crates/agentdash-application/src/shared_library/...`
- `packages/app-web/src/generated/...`

验收：

- API 返回标准 source/listing/detail。
- import 创建或更新 `LibraryAsset`，写入稳定 `source_ref`。
- `LibraryAsset.payload` 继续按 asset type validator 校验。

### 3. Skill Catalog Source 导入闭环

目标：

- 新增 Skill catalog provider adapter。
- 复用现有 `RemoteSkillSource` 与 Skill 文件校验能力。
- 外部 Skill listing 可导入成 `skill_template` LibraryAsset，再安装成 Project SkillAsset。

主要文件：

- `crates/agentdash-spi/src/platform/skill_source.rs`
- `crates/agentdash-infrastructure/src/skill_source/...`
- `crates/agentdash-application/src/skill_asset/service.rs`
- `crates/agentdash-application/src/shared_library/...`

验收：

- GitHub / ClawHub / skills.sh 或配置 catalog 至少有一条金线。
- 缺少 `SKILL.md`、文件过大、非法路径均被拒绝。
- 导入后的 Skill 使用现有 Marketplace install/source-status。

### 4. MCP Catalog Source 导入闭环

目标：

- 定义 MCP catalog payload / transport template。
- 导入成 `mcp_server_template` LibraryAsset。
- 安装时补齐用户参数并创建 Project MCP Preset。

主要文件：

- `crates/agentdash-contracts/src/mcp_preset.rs`
- `crates/agentdash-application/src/mcp_preset/...`
- `crates/agentdash-application/src/shared_library/...`
- `crates/agentdash-api/src/routes/mcp_presets.rs`

验收：

- 无密钥 MCP 模板可从外部来源导入并安装。
- credential/header/env/local path/private URL 规则在导入或安装阶段生效。
- 安装后 probe 能返回工具列表或明确错误。

### 5. Marketplace 前端外部来源体验

目标：

- Marketplace 页面增加“公共资源库 / 外部来源”浏览模式。
- 支持 source filter、asset type filter、搜索、详情抽屉。
- 支持导入并安装流程，复用现有 install API 和 source-status 刷新。

主要文件：

- `packages/app-web/src/features/assets-panel/categories/MarketplaceCategoryPanel.tsx`
- `packages/app-web/src/services/...`
- `packages/app-web/src/types/...`
- `packages/app-web/src/generated/...`

验收：

- 用户可从外部来源导入 Skill / MCP。
- 导入成功后 Marketplace 可显示对应 Shared Library asset。
- 安装成功后 Project source-status 与现有卡片状态一致。

### 6. 规格沉淀与集成 Review

目标：

- 更新 Shared Library / Marketplace Source / Integration API 相关 spec。
- 做一次跨 child 集成 review，确认没有绕过 Project Asset 运行事实源。

主要文件：

- `.trellis/spec/cross-layer/shared-library-contract.md`
- `.trellis/spec/backend/shared-library.md`
- `.trellis/spec/backend/capability/integration-api.md`
- `.trellis/spec/frontend/type-safety.md`

验收：

- spec 记录 Marketplace Source 设计理由、来源身份、导入与安装语义。
- check 阶段能按 spec 验证 child 实现一致性。

## 推荐顺序

1. SPI / Registry。
2. API / Contracts。
3. Skill catalog。
4. MCP catalog。
5. Frontend。
6. Spec + integration review。

Skill 与 MCP 在 API / Contracts 完成后可并行推进；前端需要等待至少一个后端金线稳定。

## 验证计划

后端：

```powershell
cargo test -p agentdash-spi -p agentdash-integration-api -p agentdash-api
cargo test -p agentdash-application skill_asset
cargo test -p agentdash-application mcp_preset
```

前端：

```powershell
pnpm --filter @agentdash/app-web typecheck
pnpm --filter @agentdash/app-web test
```

端到端：

```powershell
pnpm dev
```

手动金线：

- 外部 Skill listing -> 详情 -> 导入 -> 安装 -> Project SkillAsset 可见。
- 外部 MCP listing -> 详情 -> 参数填写 -> 导入 -> 安装 -> probe。
- 企业分发来源关闭或 asset 删除后，已安装 Project 资源仍保持当前版本，source-status 只提示来源状态。

## 风险点

- 企业分发服务返回 payload 形状不稳定：通过 typed validator 和 detail/fetch 分层控制。
- MCP 模板与用户私密连接材料混淆：通过 transport template + install input 分层控制。
- 前端形成第二套安装状态：外部 listing 只表示候选，导入后立即回到 Shared Library install/source-status。
- SPI 过早承诺过宽能力：首期 trait 只覆盖发现、详情、拉取，不承诺同步、签名或运行时执行。
