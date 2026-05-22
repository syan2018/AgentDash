# Builtin 资产版本维护与升级治理

## Goal

为 builtin / plugin_embedded 资产建立可维护版本策略：当内置资产 payload 变化并被资源市场升级消费时，资产版本号必须随之更新；安装来源、source-status 与升级提示应基于版本和 digest 一致工作，并提供测试或检查防止 payload 变更遗漏版本升级。

## Confirmed Facts

- `crates/agentdash-application/src/shared_library/seed.rs` 当前用单一 `BUILTIN_VERSION = "1.0.0"` 构造 agent / MCP / workflow / skill builtin seeds，所有 builtin asset 共享同一个版本事实。
- `BuiltinSeed` 已包含 `version`、`payload_digest` 与 typed `payload`，plugin embedded seed 已包含 `version` 与 `payload`，宿主会统一计算 plugin embedded `payload_digest`。
- `InstalledAssetSource` 已保存 `library_asset_id`、`source_ref`、`source_version`、`source_digest`、`installed_at`，Project agent / MCP preset / skill / workflow / lifecycle / VFS mount / extension installation 已有来源状态链路。
- `SharedLibrarySourceStatus` 当前只有 `up_to_date`、`update_available`、`source_missing` 三态；这三态适合作为用户可操作状态，不应扩展为暴露平台维护错误的前端状态。
- Marketplace 前端类型与 mapper 当前也只支持三态，安装摘要按 `source_missing > update_available > up_to_date` 聚合。
- `plugin_embedded` 资产的 `source_ref` 已使用 `plugin:{plugin_name}:{asset_type}:{key}`，但插件包版本与资产版本尚未作为两个不同事实明确治理。

## User Value

- 维护 builtin / plugin_embedded 资产时，payload 变化会被自动检查和启动期断言约束，减少因忘记 bump version 导致的 Marketplace 更新状态错误。
- Project 侧安装副本可以稳定展示是否落后于 Shared Library 来源，并在覆盖更新后保持内容、业务版本和 installed source 元数据一致。
- 插件内嵌资产可以随着插件生态扩展而保持可审计：插件代码版本、资产版本、payload digest 的含义清晰分离。

## Requirements

- 为所有 builtin / plugin_embedded library assets 建立 per-asset 版本事实源，版本粒度为 `asset_type + key + source_ref`，避免 payload / digest 变化时继续沿用旧版本号。
- builtin seed 构造必须从版本事实源读取版本；`payload_digest` 必须由 canonical payload 自动计算，维护者只维护版本与 payload。
- plugin embedded 资产必须继续由插件声明 asset version，由宿主补齐 plugin 名称、source_ref、digest 和 typed validator；插件包版本只用于审计，不替代 asset version。
- Workflow template、agent template、MCP server template、skill template、VFS mount template、extension template 等资产使用同一套版本治理规则；资产类型差异只体现在 payload 构造和安装 mapper。
- source-status 继续只表达用户可操作的三态；version 与 digest 的维护错误必须在 seed / startup / test 阶段 fail-fast，不返回给前端：
  - digest 相同表示已完全一致；
  - digest 不同且 current source version 高于 installed source version 表示正常可升级；
  - digest 不同但 current source version 未高于 installed source version 表示内置/插件资产版本维护错误，启动期断言失败；
  - 来源缺失、不可见或 deprecated 表示 source missing。
- 覆盖安装时，项目侧 installed source 记录必须保存 LibraryAsset 当前 version 与 digest；重复安装相同资产不得制造虚假版本推进。
- 覆盖更新必须在单个事务语义内保持 Project 资源内容、Project 资源业务版本、installed source version/digest 一致。
- 内置资产 payload 发生变更时，必须有自动化检查或测试要求维护者同步更新 version，并指出具体 `source_ref` / `asset_type` / `key`。
- 版本策略必须覆盖启动 seed、plugin embedded seed、资源市场列表、项目 source-status、install/update 结果；前端 Marketplace 保持三态消费。
- 旧的不一致数据通过 migration 或 startup repair 进入正确状态；当前预研期以正确模型收敛为优先。

## Scope Boundaries

- 本任务聚焦 Shared Library / Marketplace 安装资产的版本治理，不设计字段级 diff、三方合并或自动同步 Project 副本。
- 本任务聚焦 builtin / plugin_embedded 资产；user authored / remote imported 资产沿用现有 publish version 与 digest 规则，只复用统一 source-status 比较模型。
- 本任务可以调整后端 seed / startup / migration / DB repair，因为当前项目未上线；不为维护错误新增用户可见 API enum。
- 本任务应清理仍会绕过 Shared Library 版本状态的 builtin bootstrap/reset 路径，让公共配置资产统一从 seed 到 Marketplace install。

## Acceptance Criteria

- [ ] 存在明确的 builtin asset version manifest / typed registry / 等价结构，能按 `asset_type + key` 表达 version，并被 builtin seed 构造直接消费。
- [ ] `payload_digest` 由统一 canonical JSON sha256 规则计算；builtin manifest 不手写 digest。
- [ ] 内置资产 seed 不再共享单一 `BUILTIN_VERSION`，每个 asset 的 version 均可独立维护。
- [ ] plugin embedded seed 保持插件声明 asset version、宿主计算 digest 的边界；source_ref 仍稳定为 `plugin:{plugin_name}:{asset_type}:{key}`。
- [ ] 自动化检查覆盖 builtin 和 first-party plugin embedded seeds：payload digest 变化但 version 未提升时测试失败，并输出具体 source_ref / asset_type / key。
- [ ] 自动化检查覆盖 version 提升但 digest 未变化的情况，并将其视为无效版本推进或明确诊断。
- [ ] source-status 后端继续只返回 `up_to_date`、正常 `update_available`、`source_missing`；版本维护错误在 seed/startup/test 阶段被断言拦截，不进入 Marketplace API。
- [ ] Marketplace 卡片、详情抽屉、安装摘要无需展示版本维护错误；当后端成功启动时，前端只面对用户可操作状态。
- [ ] 覆盖更新成功后，项目侧 ProjectAgent / MCP preset / SkillAsset / VFS mount / workflow definition / activity lifecycle definition / extension installation 的 installed source version 与 digest 与 LibraryAsset 一致。
- [ ] 覆盖更新失败时，项目侧 installed source、业务资源版本号和资源内容保持事务一致。
- [ ] migration 或 startup repair 能把既有 builtin/plugin_embedded LibraryAsset 与 Project installed source 修正到新模型。
- [ ] Rust 测试覆盖 seed registry、digest snapshot、source-status 比较、install overwrite 事务一致性、plugin embedded identity 冲突。
- [ ] 前端 typecheck/test 确认 Marketplace 三态聚合行为不回退；后端维护错误不新增前端状态。
- [ ] 浏览器验证资源市场能正确显示 builtin 资产更新状态；修改一个 builtin payload + bump version 后可升级，修改 payload 不 bump version 时能被测试或启动期断言拦下。
- [ ] 补充 Trellis spec，记录内置资产版本可维护的设计理由。

## Decided Direction

- 版本维护错误不作为公开 source-status 返回给前端。它属于平台维护不变量破坏，应在 seed / startup / test 阶段 fail-fast；Marketplace 只展示用户可以理解并操作的三态。

## Notes

- 本任务是 Lifecycle Activity 重构收口后的独立治理任务。它不要求在上一 PR 中立即把所有 builtin 资产版本号翻新，但要求后续任何内置资产升级都可被审计、比较和维护。
