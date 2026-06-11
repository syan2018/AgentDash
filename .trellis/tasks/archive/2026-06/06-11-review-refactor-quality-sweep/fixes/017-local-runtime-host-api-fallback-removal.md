# FIX-017: Local runtime host API fallback 清理

## 模块

`local-runtime`

## 来源

- `research/local-runtime-followup-executable-plan.md`
- Batch G: Remove unsupported Rust host-api fallback for `runtime.invoke` / `channel_invoke`

## 更新

- `ctx.api.runtime.invoke()` 保留当前 host 内已加载 runtime action 的调用能力，未加载目标直接在 runner 内返回诊断错误。
- 未加载 runtime action 仍先执行 `runtime.invoke[:key]` 权限校验，再返回未加载诊断，避免未授权调用探测 host surface。
- `ctx.api.channels.invoke()` 与 dependency alias channel 调用保留当前 host 内已注册 channel method 的路由能力，未加载目标直接在 runner 内返回诊断错误。
- `host_api.rs` 删除 `runtime.invoke` 与 `extension.channel_invoke` Rust host API 分支，让 host API facade 只承载真实内置能力。
- host API 单测移除旧占位 fallback 断言，并新增 runner 级未加载 action/channel 回归测试。

## 涉及文件

- `crates/agentdash-local/src/extensions/host/runner/context.mjs`
- `crates/agentdash-local/src/extensions/host/host_api.rs`
- `crates/agentdash-local/src/extensions/host/tests.rs`

## 验证

- `cargo test -p agentdash-local runtime_invoke`：4 passed；主控补充权限顺序后复跑通过。
- `cargo test -p agentdash-local protocol_channel`：2 passed；主控复跑通过。
- `cargo test -p agentdash-local dependency_alias_invokes_provider_channel_in_same_host`：1 passed。
- `cargo test -p agentdash-local host_api`：12 passed；主控复跑通过。
- `cargo check -p agentdash-local`：通过。
- `git diff --check`：通过。
- Rust 命令输出存在既有 `agentdash-executor` unused import warning，与本次改动无关。

## Commit

未提交。
