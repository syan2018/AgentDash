# 实现 apply patch 流式预览执行计划

## Checklist

- [x] 梳理 `stream_mapper` 中 tool call delta、tool state、fileChange item 构造的现有测试。
- [x] 抽出/新增 preview parser：从 partial JSON draft 提取 `patch` 字段，并把 Codex apply_patch 文本转换为 `FileChangeSpec`。
- [x] 在 `AssistantStreamEvent::ToolCallDelta` 分支支持通用工具输入更新；`fs_apply_patch` 特化产出 `ItemStarted(fileChange, in_progress)`。
- [x] 确认 `ToolCallEnd` / `ToolExecutionStart` / `ToolExecutionEnd` 的现有终态映射继续覆盖同一 item。
- [x] 补 executor tests：partial patch draft preview、重复更新同 item、不可解析静默、非 patch 工具 parseable draft 更新输入预览。
- [x] 补前端 reducer 测试：同 item `item_started` 更新 fileChange 条目。
- [x] 梳理并锁定通用工具更新：`ToolCallDelta -> input preview` 与 `ToolExecutionUpdate -> output/progress` 两条路径继续可用；其它工具的专用展示/解析属于后续按工具适配，不在本次强行新增协议。
- [x] 运行聚焦测试与必要 type/check。

## Validation

- `cargo test -p agentdash-executor stream_mapper`
- `pnpm --filter app-web test -- sessionStreamReducer`
- 如只改 executor 映射，补跑相关 `connector_tests` 过滤用例。
- 如修改跨层 generated type，再运行 `cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts -- --check` 或项目 contract check；本计划默认不需要。

## Risky Files

- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs`
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts`
- `packages/app-web/src/features/session/model/sessionStreamReducer.test.ts`

## Rollback Points

- 若 partial JSON 提取器复杂度过高，先只在 `is_parseable=true` 时做预览；该版本仍能在工具执行前、完整参数生成后展示 fileChange。
- 若前端 repeated `item_started` 更新语义不稳定，后端仍保持同 item id，前端 reducer 单点修复 upsert。
