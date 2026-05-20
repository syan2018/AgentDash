# Builtin 资产版本维护与升级治理

## Goal

为 builtin / plugin_embedded 资产建立可维护版本策略：当内置资产 payload 变化并被资源市场升级消费时，资产版本号必须随之更新；安装来源、source-status 与升级提示应基于版本和 digest 一致工作，并提供测试或检查防止 payload 变更遗漏版本升级。

## Requirements

- 为所有 builtin / plugin_embedded library assets 明确版本事实源，避免 payload / digest 变化时继续沿用旧版本号。
- Workflow template、agent template、MCP preset、skill template、extension template 等内置资产应使用同一套版本治理规则；资产类型差异只体现在 payload 构造，不体现在版本判断逻辑。
- 资源市场的 `source_status` 与更新提示必须同时暴露“版本变化”和“digest 变化”的含义：
  - digest 相同表示已完全一致；
  - digest 不同且 source version 更高表示可升级；
  - digest 不同但 source version 未提高应被视为内置资产维护错误，而不是静默展示为普通升级。
- 覆盖安装时，项目侧 installed source 记录必须保存资产当前版本与 digest；重复安装相同资产不得制造虚假版本推进。
- 内置资产 payload 发生变更时，必须有自动化检查或测试要求维护者同步更新版本号，避免依赖人工记忆。
- 版本策略应覆盖启动 seed、plugin embedded seed、资源市场列表、项目 source-status 和 install/update 结果。
- 不引入兼容兜底路径；当前项目处于预研期，旧的不一致数据应通过 migration / startup repair 进入正确状态。

## Acceptance Criteria

- [ ] 存在明确的 builtin asset version manifest 或等价结构，能按 asset key / type 表达版本。
- [ ] 内置资产 seed 构造使用该版本事实源，不再把所有资产固定为不可维护的同一版本。
- [ ] 当内置资产 payload digest 变化但版本未提升时，测试或检查会失败，并指出具体 asset key。
- [ ] 资源市场 source-status 能区分 `up_to_date`、正常 `update_available` 和“内置资产版本维护错误”的诊断状态或后端错误。
- [ ] 覆盖更新成功后，项目侧 workflow / lifecycle / agent / skill / preset / extension 的 installed source version 与 digest 与 library asset 一致。
- [ ] 覆盖更新失败时，项目侧 installed source、业务资源版本号和资源内容保持事务一致。
- [ ] 浏览器验证资源市场能正确显示 builtin 资产更新状态；修改一个 builtin payload + bump version 后可升级，修改 payload 不 bump version 时能被测试/检查拦下。
- [ ] 补充 Trellis spec，记录内置资产版本可维护的设计理由。

## Notes

- 本任务是 Lifecycle Activity 重构收口后的独立治理任务。它不要求在上一 PR 中立即把所有 builtin 资产版本号翻新，但要求后续任何内置资产升级都可被审计、比较和维护。
