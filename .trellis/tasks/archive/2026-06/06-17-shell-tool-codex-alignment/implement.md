# Shell 工具扩展与 Codex 行为对齐 Implement Plan

## Checklist

- [ ] 升级 Codex git dependency 到 `rust-v0.140.0`，运行 `cargo update -p codex-app-server-protocol` 或等价更新并验证锁文件。
- [ ] 为可复用 Codex 轻量 crate 做最小编译评估：
  - [ ] 添加并验证 `codex-utils-pty`。
  - [ ] 添加并验证 `codex-utils-output-truncation`。
- [ ] 参考 `codex-exec-server` 的 `ExecParams` / `ReadParams` / `WriteParams` / `TerminateParams` 行为，但不默认引入其完整 crate 依赖。
- [ ] 盘点现有 shell / terminal / output truncation / process spawn 中由项目自行维护的重复 feature；凡是可由轻量依赖或统一 `ShellSessionManager` 承接的实现，直接删除旧路径，不保留双轨维护。
- [ ] 在 `agentdash-relay` 设计并实现 shell start/read/input/terminate 协议 DTO、relay message variants 和 round-trip tests。
- [ ] 在 `agentdash-local` 新增 `ShellSessionManager`：
  - [ ] spawn pipe/PTY process。
  - [ ] stdout/stderr/pty 输出 watcher。
  - [ ] seq retained output buffer。
  - [ ] read wait / input / terminate。
  - [ ] exit watcher、trailing output grace、retention、prune。
- [ ] 将现有 `ToolCommandHandler::handle_tool_shell_exec` 改为新 session manager 的 start + initial wait。
- [ ] 调整 `TerminalManager` 或引入共享 process substrate，让 shell session 可以被 terminal tab 打开并复用同一 output/state。
- [ ] 调整 `agentdash-api` relay handler 和 registry 使用方式，确保 pending 只覆盖单次 start/read/input/terminate RPC。
- [ ] 调整 `agentdash-application` 的 VFS `exec` adapter 和工具结果组装，输出 running handle、partial output、exit state、truncation metadata。
- [ ] 更新前端 generated types、terminal store、command execution card 和 terminal tab，使 command card 与 terminal tab 指向同一真实 shell session。
- [ ] 删除被新模型替代的旧 shell timeout/grace 特化逻辑，保留必要的短 RPC timeout。
- [ ] 补充测试并运行聚焦验证。

## Validation Commands

- `cargo check -p agentdash-relay -p agentdash-local -p agentdash-api -p agentdash-application`
- `cargo test -p agentdash-relay shell`
- `cargo test -p agentdash-local shell`
- `cargo test -p agentdash-api relay`
- `cargo test -p agentdash-application exec`
- 前端类型生成命令按仓库现有脚本执行。
- 受影响前端测试按文件聚焦运行：terminal store、command execution card、terminal tab。

## Risk Points

- `codex-exec-server` 公开 API 足够贴近，但依赖面明显偏重；实现阶段只参考其行为，避免默认引入完整运行时依赖。
- Windows PTY / pipe 行为差异会影响 stdin、EOF、exit code 和尾部输出；测试需要覆盖 Windows 当前开发环境。
- relay event 到达顺序与 response 到达顺序可能交错；read API 必须以 seq 和 retained buffer 为事实源。
- 前端当前 `outputBuffers` 是无界字符串，需要配合 retained output metadata 改为有上限或按 session chunk 合并。
- 现有 terminal exit code 为空，需要新 shell session state 提供可靠 exit code。
- 删除旧 feature 时要确认调用方已迁移到新事实源，避免留下不可达代码和影子状态。

## Rollback Points

- Codex dependency 升级单独提交或单独 diff，可在编译失败时独立回退。
- 新 relay message variants 与旧 shell handler 改造分阶段提交；在切换 VFS adapter 前可保持旧 shell_exec 测试通过。
- 前端 terminal 复用可在后端 shell session 稳定后再接入。

## Review Gate Before Start

- PRD、design、implement 已更新到当前决策。
- 用户确认启动实现。
- 任务通过 `task.py start` 激活后再进入代码实现。
