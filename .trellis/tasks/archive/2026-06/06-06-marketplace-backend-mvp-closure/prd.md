# Marketplace 后端 MVP 收束

## 背景

父任务 `.trellis/tasks/06-05-external-marketplace-sources/` 已完成 Marketplace Source SPI、外部 Marketplace API / Contracts，以及 Skill URL Import 收束到 Shared Library 的基础工作。当前后端还缺少一个真正能代表 MVP 完成度的闭环：外部 MCP 来源需要从 catalog listing/detail/fetch 标准化导入成 `mcp_server_template`，并在安装阶段生成可运行的 Project MCP Preset。

前端外部来源体验本轮明确不处理。后端任务的价值是把 MCP / Skill 两类后端合同、版本来源、导入语义和安装语义收束稳定，让后续前端同事可以基于明确 API 和 generated contracts 接入。

## 已确认事实

- `MarketplaceSourceProvider` SPI 已存在，支持 source descriptor、cursor 分页、detail 和 fetched payload。
- `/api/marketplace/sources`、`/api/marketplace/external-assets`、`import`、`refresh` API 已存在。
- 外部 import 已能写入 `LibraryAsset(source=remote_imported)`，`source_ref` 使用 `market:{source_key}:{asset_type}:{external_id}`。
- `skill_template` URL Import 已收束为：远端 fetch -> materialize `LibraryAsset` -> Shared Library install -> Project `SkillAsset` 写入 `InstalledAssetSource`。
- 当前 `mcp_server_template` payload 已有 `transport`、`route_policy`、`parameter_schema`、`capabilities` 字段，但安装逻辑仍直接复制具体 `McpTransportConfig`，没有把参数 schema 与安装输入解析为最终 Project MCP Preset。
- 现有 generic Shared Library install request 只包含 `library_asset_id`、`target_key`、`overwrite`，没有资产类型专属安装选项。
- Project MCP Preset 运行事实仍应是安装后的 Project 资源，运行时不直接读取 Marketplace listing 或 `LibraryAsset.payload`。

## 目标

R1. 完成外部 MCP catalog 后端闭环：provider 返回的 MCP listing/detail/fetch payload 可以导入为 `LibraryAsset(asset_type=mcp_server_template, source=remote_imported)`，并通过 Shared Library install 生成带 `InstalledAssetSource` 的 Project MCP Preset。

R2. 明确 `mcp_server_template` 的模板语义：公共 LibraryAsset payload 只保存无密钥、可校验的连接模板、参数 schema、能力摘要和远端版本事实；用户安装输入只在安装阶段参与生成 Project MCP Preset。

R3. 扩展 Shared Library install 合同，使 MCP 模板安装可以携带类型化安装选项。非 MCP 资产不得接受 MCP 安装选项；MCP 模板缺少必需参数或参数类型不合法时返回字段级可定位错误。

R4. 保持外部来源分页、版本、digest、refresh 语义不变：远端 version/digest 只用于导入和显式刷新提示，不静默修改 Project 资源。

R5. 保持 Skill 后端闭环：现有 GitHub / ClawHub / skills.sh URL Import 继续走 Shared Library 写入链路；外部 Marketplace 通用 import 对 `skill_template` 的支持必须通过测试保留。

R6. 提供后端可验证的来源样例或测试 provider，覆盖 MCP listing -> detail -> import -> install 的金线路径，方便后续前端任务在没有企业来源时验证合同。

R7. 更新 backend / cross-layer spec，记录外部 Marketplace 后端 MVP 的最终合同和原因。

## 范围

本任务包含：

- Domain / contract / API / application 层的 MCP 模板安装合同。
- 外部 Marketplace MCP import 的 payload validator、source/version/digest 语义和错误映射。
- Shared Library install 对 MCP 参数输入的支持。
- 后端测试 provider 或 first-party fixture，用于验证后端金线。
- Rust contracts 生成的前端 DTO 文件更新。
- 后端和跨层规格文档更新。

本任务不包含：

- Marketplace 前端外部来源页面、详情抽屉或导入交互。
- Capability Pack 市场化。
- 用户自定义外部 source 管理、配置式 HTTP catalog、签名、自动同步或远程自动升级。
- 未经连接/凭据治理设计的外部密钥分发。
- 动态加载第三方 native integration。

## Acceptance Criteria

- [ ] 外部 MCP fetched payload 经过 typed validator 后可写入 `remote_imported` `mcp_server_template` LibraryAsset。
- [ ] `mcp_server_template` payload 明确区分公共模板和安装输入，公共 payload 不保存 credential/header/env 值、本机路径、localhost 或私网 URL 等连接材料。
- [ ] Shared Library install 支持 MCP 模板安装选项，能够把参数输入解析为最终 `McpTransportConfig` 并创建 Project MCP Preset。
- [ ] 缺少必需参数、参数类型错误、未知参数、模板解析后仍有未绑定占位符时，后端返回明确错误。
- [ ] 外部 MCP import/install 写入的 Project MCP Preset 带 `InstalledAssetSource`，source-status 能按 LibraryAsset version/digest 提示更新。
- [ ] `POST /api/marketplace/external-assets/refresh` 对 MCP asset 继续只比较远端 listing 与本地 LibraryAsset，不修改 Project MCP Preset。
- [ ] `skill_template` 外部 import 和现有 Skill URL Import 的 Shared Library 写入链路有回归测试覆盖。
- [ ] contracts 生成产物反映新的 install request / MCP template DTO；前端任务无需猜测后端 payload 形态。
- [ ] spec 更新覆盖 Marketplace Source、Shared Library install、MCP template 参数语义和前端交接边界。
- [ ] 任务进入实现前，`design.md` 与 `implement.md` 已给出可交给 subagent 的文件清单、执行顺序和验证命令。

## 开放问题

暂无阻塞规划的问题。默认决策是后端 MVP 只完成 MCP / Skill 的标准化导入和安装合同，前端实现独立拆给后续任务。
