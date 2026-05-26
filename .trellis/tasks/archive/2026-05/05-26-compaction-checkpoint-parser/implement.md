# 实施计划

## Checklist

1. 挂载 `compaction_checkpoint` 内部模块，并实现统一类型与 JSON 读取 helper。
2. 实现 projection 侧解析：
   - segment metadata 优先；
   - compaction record fallback；
   - summary entry 构造统一入口；
   - source range、version、segment ownership 校验。
3. 实现 event 侧解析：
   - 从 `context_compacted` payload 解析 checkpoint；
   - 提供 latest checkpoint discovery；
   - 提供 continuation 裁剪 helper。
4. 改造 `ContextProjector`：
   - 删除本地 `checkpoint_messages_compacted` / `checkpoint_compacted_until_ref` / summary entry 构造逻辑；
   - 保留 token estimate 函数；
   - 使用 parser 输出的 entries。
5. 改造 `continuation.rs`：
   - 删除私有 `CompactionCheckpoint` 和 `extract_compaction_checkpoint`；
   - 使用 parser 的 latest checkpoint 和 apply helper。
6. 补齐测试并确保现有回归通过。

## Validation

- `cargo test -p agentdash-application compaction_checkpoint`
- `cargo test -p agentdash-application compaction -- --nocapture`
- `cargo test -p agentdash-infrastructure compaction_projection -- --nocapture`
- `cargo test -p agentdash-contracts projection`
- `pnpm --filter app-web test -- SessionProjectionView`
- `pnpm --filter app-web test -- SessionChatView`
- `git diff --check origin/main...HEAD`

## Risk Points

- `ContextProjector` 和 `continuation.rs` 对 `compacted_until_ref` 缺失的处理不同：projection 可展示 `None`，continuation 不应猜测裁剪边界。
- `cargo fmt` 会格式化额外 Rust 文件；只保留与本任务相关 diff。
- `.trellis/tasks/archive/.../arch-review.md` 已移动为本任务补充资料，提交时只 stage 新任务目录中的副本。
