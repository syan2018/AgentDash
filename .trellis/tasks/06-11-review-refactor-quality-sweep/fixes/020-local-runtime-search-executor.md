# FIX-020: local-runtime SearchExecutor 与共享文件发现策略

## 模块

`local-runtime`

## 来源

- `research/local-runtime-followup-executable-plan.md`
- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/backend/quality-guidelines.md`

## 更新

- 新增 `SearchExecutor`，集中处理 ripgrep 探测、ripgrep 参数策略、30 秒搜索超时和 `--json` match 解析。
- `ToolExecutor::search()` 保留 workspace root validation，搜索路径解析与执行委托给 `SearchExecutor`。
- 新增 `FileDiscoveryPolicy`，统一 hard exclude、builtin noise、workspace ignore 遵循规则，供 `file_list` 与 search 共用。
- 保持 ripgrep 不可用时 fail-fast，返回 `ToolError::Io(NotFound)`。
- search 相关测试从 `tool_executor` 测试模块迁移到 `search_executor` 测试模块。

## 涉及文件

- `crates/agentdash-local/src/file_discovery_policy.rs`
- `crates/agentdash-local/src/search_executor.rs`
- `crates/agentdash-local/src/tool_executor.rs`
- `crates/agentdash-local/src/lib.rs`

## 验证

- `cargo fmt -p agentdash-local`：通过。
- `cargo fmt --check -p agentdash-local`：通过。
- `cargo test -p agentdash-local tool_executor`：主控在 workflow 并发提交后复跑通过，21 passed。
- `cargo test -p agentdash-local search_requires_ripgrep_when_unavailable`：主控在 workflow 并发提交后复跑通过，1 passed。
- `cargo check -p agentdash-local`：主控在 workflow 并发提交后复跑通过。
- Rust 命令输出存在既有 `agentdash-executor` unused import warning，与本次改动无关。

## Commit

未提交；本轮按要求只完成代码与记录更新。
