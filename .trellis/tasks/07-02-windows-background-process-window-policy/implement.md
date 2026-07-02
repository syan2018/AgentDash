# 实现计划

## Checklist

- [x] 暂停 Windows Service / runner 宿主方向，不作为本任务实现路径。
- [x] 建立后台进程启动 substrate：visibility、domain、std/tokio command helper、Windows hidden policy、diagnostics。
- [x] 将 `agentdash-local` 后台执行路径迁移到 substrate。
- [x] 保留 `pnpm dev` / `scripts/` 可见编排，不纳入 hidden guard。
- [x] 将 `agentdash-executor` 与 `agentdash-infrastructure` 的后台 spawn 点迁移到等价 substrate 或明确 shared helper。
- [x] 将 `codex-utils-pty` 回到上游 git 依赖，窗口策略根因收束在 AgentDash 外围后台 spawn substrate，不把 vendor 作为修复点。
- [x] 新增 guard 脚本，扫描后台 Rust 执行面并通过。
- [x] 更新 spec：记录本机执行 substrate 后台静默的原因。

## Notes

- `vendor/codex-utils-pty` 不再作为 workspace patch 参与构建；Codex PTY / ConPTY 使用上游实现，AgentDash 只治理外围后台 pipe/stdio/probe/sidecar/function/Codex bridge spawn。
- Guard 扫描 `crates/agentdash-local`、`crates/agentdash-local-tauri`、`crates/agentdash-executor`、`crates/agentdash-infrastructure` 和 `crates/agentdash-process`，不扫描 vendor。

## Validation Commands

```powershell
cargo fmt --all
cargo check -p agentdash-local
cargo check -p agentdash-executor
cargo check -p agentdash-infrastructure
node scripts/check-background-process-spawn.js
cargo clippy --no-deps -p agentdash-process -p agentdash-local -p agentdash-local-tauri -p agentdash-executor -p agentdash-infrastructure -- -D warnings
```

## Risk Areas

- 不同 crate 之间依赖方向不同，可能需要先局部 substrate、后续再抽共享 crate。
- guard 需要精确 allowlist，避免把测试、vendored 低层和开发编排误报。
- diagnostics 需要脱敏 args/env，不能记录 token-bearing values。
