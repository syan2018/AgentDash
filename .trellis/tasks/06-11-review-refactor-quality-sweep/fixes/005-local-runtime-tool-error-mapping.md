# FIX-005: local-runtime ToolError 映射收敛

## 模块

`local-runtime`

## 来源

- `reviews/004-local-runtime.md`
- `research/local-runtime-executable-plan.md`
- worker: `019eb2b4-d727-7f90-99bf-121962c0474b`

## 更新

- 新增 `tool_error_to_relay_error`，统一 tool call handler 的错误出口。
- 将 workspace 越界、非法路径、超时、IO、patch apply 错误映射到稳定 `RelayErrorCode`。
- 各 tool response 不再散落 `io_error` / `runtime_error` 的临时转换。

## 涉及文件

- `crates/agentdash-local/src/handlers/tool_calls.rs`

## 验证

- `cargo test -p agentdash-local tool_error_to_relay_error`：3 passed。
- `cargo check -p agentdash-local`：通过；仅剩既有 `agentdash-executor` unused import warning。
- `cargo fmt -p agentdash-local`：通过。
- 汇合验证：`cargo test -p agentdash-local`：86 passed。

## Commit

待提交。
