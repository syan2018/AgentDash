# 工作项 4：强制更新阻断与只读诊断

## Goal

当桌面端版本低于服务端显式配置的 `min_desktop_version` 时，桌面端进入强制更新阻断态，阻止使用依赖兼容协议的核心能力，同时保留更新、重试、退出和只读诊断入口。

## Scope

- 强制更新检查优先于 runtime auto-connect、登录后的 runtime claim、Relay 连接和会话入口。
- 未显式配置 `min_desktop_version` 时不得触发强制更新。
- 阻断态允许查看只读诊断/日志。
- 阻断态不得允许启动 runtime、修改设置、进入会话或产生远端/本机运行侧副作用。
- 与更新下载/安装流程联动，引导用户完成更新。

## Deliverables

- 桌面启动早期的版本策略检查。
- 强制更新阻断状态模型。
- runtime auto-connect / claim / Relay / 会话入口 gating。
- 只读诊断视图或诊断入口。
- 测试覆盖显式最低版本、未配置最低版本、低版本阻断、同版本/高版本放行。

## Checkpoints

- [x] 低于 `min_desktop_version` 时不自动连接本机 runtime。
- [x] 低于 `min_desktop_version` 时不发起 runtime claim。
- [x] 低于 `min_desktop_version` 时不建立 Relay 连接。
- [x] 低于 `min_desktop_version` 时不能进入需要协议兼容的会话工作流。
- [x] 阻断态能检查更新、下载更新、安装更新、重试和退出。
- [x] 阻断态能查看只读诊断/日志，且不会触发运行侧副作用。
- [x] 未配置最低版本时本地调试不被阻断。

## Suggested Validation

- 桌面状态机测试。
- 本机 runtime bridge 相关测试。
- `pnpm run desktop:check`
- 必要时用 mock 云端响应做端到端验证。
- 按 `deploy/runbooks/desktop-update-e2e.md` 验证旧客户端在显式 `min_desktop_version` 下阻断 runtime start/restart 和 mutation commands，同时保留 update/status/quit/只读诊断。

## Check Result

- 已通过 `cargo test -p agentdash-local-tauri desktop_update`、`cargo check -p agentdash-local-tauri` 和 `pnpm run desktop:check`。
- Rust command 层阻断 runtime start/restart；Web bridge 自动连接在前端也读取 gate snapshot 后放行。
- 强制更新阻断的真实旧版本安装验证流程已记录到 `deploy/runbooks/desktop-update-e2e.md`，后续在企业发布环境执行。
