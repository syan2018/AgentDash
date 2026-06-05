# PRD · Enterprise Marketplace API 与 Contracts

## 背景

父任务 `06-05-external-marketplace-sources` 已确定外部市场来源是 Marketplace 的发现入口，不是运行事实源。`06-06-marketplace-source-spi-registry` 已实现 Host Integration 注册和 provider registry。本 child 负责把 registry 暴露为后端 API 与前后端共享 contracts，并建立通用导入/刷新入口。

本 child 不实现具体 Skill catalog adapter、GitHub URL Import 收束、MCP 参数化安装或前端页面。它只要求 provider 返回的 fetched payload 已经能通过 `LibraryAssetPayload` typed validator。

## 用户价值

- 前端和后续业务 child 可以通过稳定 API 浏览外部来源、分页查询候选、查看详情。
- 外部候选可先导入为 Shared Library `LibraryAsset(source = remote_imported)`，再由现有 install 入口安装到 Project。
- 远端 `version/digest` 有统一合同，后续刷新和可更新提示不需要每类资产重造协议。

## 确认事实

- `ServiceSet.marketplace_source_providers` 已持有 Host Integration 收集后的 providers。
- `agentdash-contracts` 负责生成前端 TypeScript contract。
- `LibraryAssetRepository` 已支持 `find_by_identity` / `upsert`，`LibraryAsset::new` 会按 asset type 校验 payload。
- `LibraryAssetSource` 已有 `remote_imported`。
- Shared Library install/source-status 已存在，Project 运行事实不应直接读取外部 listing。

## 目标

R1. 在 `agentdash-contracts` 新增 external marketplace DTO，并生成前端 TS contract。

R2. 新增 API：

```text
GET  /api/marketplace/sources
GET  /api/marketplace/external-assets?source_key=&asset_type=&query=&cursor=&limit=
GET  /api/marketplace/external-assets/{source_key}/{external_id}
POST /api/marketplace/external-assets/import
POST /api/marketplace/external-assets/refresh
```

R3. source/list/detail API 从 `ServiceSet.marketplace_source_providers` 读取 provider，保留 cursor 分页、远端 version/digest 和 listing source identity。

R4. import API 调用 provider `fetch_asset_payload`，把 fetched typed payload 创建或更新为 `LibraryAsset(source = remote_imported)`；`source_ref` 使用 `market:{source_key}:{asset_type}:{external_id}`。

R5. import 写入必须通过 `LibraryAsset` / `LibraryAssetPayload` validator，不接受前端 raw payload。

R6. refresh API 建立显式远端版本检查合同，返回本地 LibraryAsset 与远端 listing 的 version/digest 比较结果，不修改 Project 资源。

R7. 错误映射清晰：未知 source、未知 external asset、provider BadRequest/Unavailable/Internal 和非法 payload 都有可理解的 HTTP 错误。

## 非目标

- 不实现真实 Skill catalog provider。
- 不改 Project Skill URL Import。
- 不扩展 MCP install 参数输入。
- 不实现前端 Marketplace UI。
- 不增加 marketplace source cache 持久表。

## 验收标准

- [ ] `agentdash-contracts` 新增 DTO 并生成前端 TS 文件。
- [ ] sources API 返回 first-party empty source descriptor。
- [ ] external assets API 支持 `source_key`、`asset_type`、`query`、`cursor`、`limit` 并返回 `next_cursor`。
- [ ] detail API 校验 source 存在并透传 provider detail。
- [ ] import API 只从 provider fetch payload，创建/更新 `LibraryAsset(source = remote_imported)`，写入稳定 `source_ref`。
- [ ] import API 对 provider 返回的 unsupported asset type 或非法 payload 返回错误。
- [ ] refresh API 不修改 Project 资源，只返回远端版本比较结果。
- [ ] API tests 覆盖成功、未知 source、provider not found、分页参数透传、import validator 错误。

## 开放问题

暂无阻塞问题。真实 Skill/MCP provider 的 payload 形状和 materializer 由后续 child 细化。
