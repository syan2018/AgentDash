# Native Runtime Adapter 与 Clean Agent Core

## Goal

将现有 Native/Pi 执行路径改造成统一 Runtime contract 的 reference adapter，并把 Agent Core 清理成 provider-neutral provider/tool loop。

## Depends On

- `02-managed-runtime-kernel`
- `03-business-agent-surface`
- `04-integration-driver-host`

## Parent Design

- `../../design.md` 第 10、13、14 节
- `../../research/agent-core-executor.md`
- `../../research/hook-runtime-layering.md`

## Requirements

- Native service通过Integration contribution与Driver SPI接入。
- 映射 Thread/Turn/Item/Interaction、ContextEnvelope、ToolCatalog与hooks/mailbox。
- Agent Core移除AgentDash lifecycle prompt/projection、runtime compaction policy、Codex DTO与repository依赖。
- Core保留 provider/tool loop、provider-neutral structured IO、cancel与纯summarization primitive。
- Native adapter实现exact context export/import、idempotent activation、managed compaction与hot tool revision ack。
- Native adapter把受支持的inner-loop HookPlan映射为显式Agent Core delegate facets，并返回applied hook plan revision/digest。
- Native成为L4 managed context behavior reference。
- 删除对应Pi AgentConnector旧入口、重复restore/context policy路径。

## Acceptance Criteria

- [x] Clean Core可在不依赖AgentRun、repository、Codex、Backbone的测试中运行。
- [x] Native restart/restore/fork/compaction保持checkpoint revision与digest。
- [x] tool surface update返回真实applied revision，不存在default success。
- [x] inner hook capability只在实际Core hook point存在时声明。
- [x] provider/tool/stop callback在正确因果点等待typed decision；Core不查询Workflow、Rhai、HookDefinition或repository。
- [x] old Pi connector和runtime-session launch路径删除。
- [x] Native通过common与L4 adapter conformance suite。
