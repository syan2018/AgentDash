# Plugin Extension Asset 化

## Goal

把 Plugin Extension API 的用户可感知扩展能力收敛到 Shared Library / Marketplace 资产体系：native plugin 可以贡献内嵌配置资产，用户通过 Marketplace 显式安装；slash command、runtime flag、extension message 等运行时扩展以 `extension_template` 资产进入 Project 与 session construction 管线。

## Background

- `04-12-plugin-extension-api` 规划了 `registerCommand`、`registerFlag`、`CustomMessage<T>` 三类插件扩展能力。
- 该任务的动态安装讨论已经把扩展拆成 Native Host Plugin、Runtime Extension Asset、External Extension Service、Frontend Extension Surface 四层。
- 现有 `AgentDashPlugin` 是启动期 Rust SPI，适合管理员安装、高权限能力、重启后生效。
- 现有 Shared Library / Marketplace 已能承接 Agent/MCP/Workflow/Skill 模板资产，但尚不能表示 plugin-contributed asset 或 runtime extension manifest。

## Foundational Principles

- Native Host Plugin 不追求普通用户热加载；管理员安装、服务重启生效是宿主级扩展边界。
- 用户可动态启用的能力应数据化为 Runtime Extension Asset，经 Marketplace 安装到 Project。
- Plugin 不直接修改 Project 运行配置；它贡献 Shared Library 资产，用户显式安装后才影响 Project。
- 工具扩展优先通过 MCP / external service 描述和调用，不把任意第三方代码热插入主服务。
- 前端扩展先使用 schema-driven renderer，不开放任意 React bundle 热加载。
- 运行路径不能直接消费未校验的 `LibraryAsset.payload`；必须进入类型化 Project 安装态或 session construction 投影。

## Requirements

1. Native plugin 可以在启动期声明内嵌 Shared Library assets。
2. Plugin 贡献的 asset 进入统一 seed/upsert 流程，出现在 Marketplace 中，并保留清晰来源：
   - 来源能区分 builtin、user_authored、remote_imported、plugin_embedded。
   - `source_ref` 能稳定定位 plugin name、asset key、version/digest。
3. Plugin 贡献 asset 与 builtin/user asset 使用同一套 payload validator、LibraryAsset identity、install、source-status 机制。
4. 新增 `extension_template` asset type，用于描述 runtime extension manifest。
5. `extension_template` payload 至少支持：
   - slash command definition
   - runtime flag definition
   - extension message type / default schema-driven renderer declaration
   - capability directives
   - 可选引用 skills / mcp presets / hook presets 的占位结构
6. Project 安装 `extension_template` 后形成明确的 Project 安装态，能启用/禁用，并记录 `InstalledAssetSource`。
7. 新 session construction 能读取启用的 Project extension installations：
   - slash command 出现在 `/` 菜单或命令注册结果中。
   - runtime flag 默认值进入 session flag state。
   - extension message 类型进入前端可识别的 schema-driven renderer registry。
8. 与 `04-12-plugin-extension-api` 保持一致：native trait 仍可作为管理员级 SPI，但用户动态扩展主路径是 asset / manifest。

## Out of Scope

- Rust dynamic library 热加载。
- 任意 TypeScript / React UI bundle 动态加载。
- 外部 marketplace 网络同步与签名包分发。
- 运行中 session 的无提示热更新；第一版可以只保证新 session 生效。
- 完整 hook preset DSL、MCP runtime discovery 刷新和 capability delta 热切换。

## Acceptance Criteria

- [ ] `AgentDashPlugin` 或相邻 SPI 能声明 plugin embedded library asset seeds。
- [ ] first-party plugin 提供至少一个 plugin embedded asset 示例，seed 后能在 Marketplace 中看到。
- [ ] `library_assets.source` 支持并持久化 plugin embedded 来源，DB check constraint 和 DTO/前端类型同步更新。
- [ ] 新增 `extension_template` asset type，后端 payload validator 能接受合法 manifest 并拒绝非法 manifest。
- [ ] Marketplace 能展示 `extension_template`，并能安装到 Project。
- [ ] Project extension installation 记录 `InstalledAssetSource`，source-status 能反映版本更新或来源缺失。
- [ ] 新 session 能读取已启用 extension installation，并至少打通 slash command + runtime flag 的最小链路。
- [ ] Extension message 支持默认 schema-driven 展示，不需要动态前端代码。
- [ ] 后端测试覆盖 plugin seed 聚合、冲突、extension payload validation、Project 安装态、session construction 投影。
- [ ] 前端 typecheck/test 通过，并覆盖 extension template 展示或 mapper。

## Open Questions

- `extension_template` 第一版安装时，是否只创建 Project extension installation，还是允许同步展开 skill / mcp / workflow 子资产；推荐第一版只做 installation，引用现有资产展开留给后续。
