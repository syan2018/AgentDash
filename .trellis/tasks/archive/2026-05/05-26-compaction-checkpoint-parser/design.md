# 设计：上下文压缩 checkpoint 解析层

## Architecture

新增 `crates/agentdash-application/src/session/compaction_checkpoint.rs`，作为 application 内部纯业务归一层。它不读写数据库、不触发 compaction、不负责 token 估算，只把持久化记录和事件 payload 解释成统一 checkpoint 对象，并提供 summary entry 与 continuation 裁剪 helper。

核心类型：

- `CompactionCheckpoint`：承载 summary、tokens_before、messages_compacted、compacted_until_ref、source_range、first_kept_event_seq、timestamp、compaction_id、segment_id、projection_version、provenance。
- `CompactionCheckpointProvenance`：记录来源为 projection segment、compaction record 或 context event，以及 trigger / strategy / phase 等审计字段。
- `CompactionCheckpointError`：覆盖非法 source range、非法 message ref、segment / compaction 版本不一致、segment 归属不一致等当前正确状态错误。

## Data Flow

- Durable projection restore：
  `ContextProjector` 读取 head、active compaction、segments 后，调用 parser 从每个 summary segment 生成 checkpoint，再由 checkpoint 生成 `ProjectedEntry`。如果没有可用 segment，则使用 compaction record fallback checkpoint。
- Event continuation restore：
  `continuation.rs` 调用 `latest_context_compacted_checkpoint(events)` 从最新 `context_compacted` event payload 生成 checkpoint，再调用 `apply_checkpoint_to_projected_entries(raw_entries, checkpoint)`。
- Suffix restore：
  `ContextProjector` 继续基于 compaction 的 `first_kept_event_seq` / `source_end_event_seq + 1` 读取 suffix；parser 提供同等语义 helper，但不直接读取事件。

## Contracts

- Parser 输入仍是现有 `SessionCompactionRecord`、`SessionProjectionSegmentRecord`、`PersistedSessionEvent` 和 `serde_json::Value`。
- Parser 输出仅在 application crate 内部使用，不进入 `agentdash-spi`。
- `ProjectedEntry` 的 origin、synthetic、source_range、provenance 由 checkpoint 统一填充。
- `compacted_until_ref` 可为 `None`；projection summary 可以保留 `None`，但 continuation 裁剪只有在存在有效 boundary 时应用。

## Trade-offs

- 不把 parser 放入 SPI：避免把 application session 恢复语义固化成持久化接口。
- 不把 token estimate 搬入 parser：token estimate 是 envelope 构建职责，parser 只表达 checkpoint 边界。
- 对当前正确状态做结构校验，不做多版本兼容；项目未上线，错误 draft 数据应重建。
