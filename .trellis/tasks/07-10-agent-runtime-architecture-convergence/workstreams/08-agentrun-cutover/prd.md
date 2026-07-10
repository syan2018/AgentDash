# AgentRun Runtime 切换与旧架构清理

## Goal

将 Application/API/UI 全量切换到 AgentRunRuntime facade 与 Runtime snapshot/events，并删除旧 runtime-session、AgentConnector、Backbone双事实、硬编码 composition 和旧 persistence schema。

## Depends On

- `02-managed-runtime-kernel`
- `03-business-agent-surface`
- `04-integration-driver-host`
- `05-native-runtime-adapter`
- `06-codex-runtime-adapter`
- `07-relay-runtime-wire`

## Parent Design

- `../../design.md` 第 4.1、13、14、17 节
- `../../implement.md` 第 10 节

## Requirements

- 实现AgentRunRuntime inspect/send/compact/steer/interrupt/resolve/read facade。
- AgentRun/AgentFrame/mailbox/product receipt映射到canonical Operation/Thread。
- API/UI消费CommandAvailability、profile provenance、semantic strength与durable cursor。
- 删除executor/connector type UI/API分支。
- 删除`application-runtime-session`、pass-through bridge、重复launch classification。
- 删除AgentConnector/Composite/ConnectorCapabilities/default no-op。
- 删除Backbone双事实、Relay旧协议、旧表/字段与compatibility path。
- 更新最终Trellis architecture/session/capability/backbone/runtime gateway specs。
- 执行workspace质量门禁与代表性E2E。

## Acceptance Criteria

- [ ] Application只依赖AgentRun facade/owned Runtime contract。
- [ ] 生产composition不存在Pi/Codex/Relay connector硬编码。
- [ ] UI命令只由bound profile + session state推导。
- [ ] 搜索不到旧runtime-session、connector capability OR、false success与vendor DTO泄漏。
- [ ] 新数据库schema是唯一读写路径，无dual write、fallback或兼容层。
- [ ] Native、Codex、Enterprise remote代表性E2E通过；首期不存在ACP driver/endpoint生产路径。
- [ ] 最终spec只记录目标职责、语义不变量与选择依据。
