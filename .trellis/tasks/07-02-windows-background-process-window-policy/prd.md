# 收束 Windows 后台进程窗口策略

## Goal

收束 AgentDash 本机执行 substrate 在 Windows 桌面环境中的 console 窗口泄漏问题，明确为什么 relay/MCP/terminal/tool 等后台触发会出现 PowerShell/cmd 弹窗，并让后台执行只通过可审计、可隐藏、可测试的进程启动边界运行。

## Problem Statement

问题不是“runner 是否应作为 Windows Service”，也不是“MCP HTTP/HTTPS transport 会启动 PowerShell”。HTTP/HTTPS MCP 分支只构造 `reqwest::Client` 和 streamable-http worker，不直接启动本机进程。

真实问题是：AgentDash 当前本机执行能力分散在多个 crate 和外部库的进程启动点里，GUI/桌面宿主、relay handler、PTY helper、stdio transport、workspace probe、function runner、sidecar 管理命令之间没有一个单一执行 substrate 约束。Windows GUI 宿主启动 console 子进程时，只要某个路径没有使用无窗口创建策略，就会在用户桌面弹出窗口。

Codex/同类本机执行工具通常不会出现这个问题，原因不是它们一定使用 Windows Service，而是它们把本机执行收敛在一个明确 substrate：命令执行、PTY、stdio MCP、sandbox/exec server、集成 terminal 都通过受控进程创建边界或嵌入式终端面，而不是让每个功能随手 `Command::new`。

## Requirements

- 先停止把 Windows Service 作为本问题的主解；Service 是部署/宿主策略，不是本机执行窗口泄漏的根因修复。
- 明确区分两类进程：
  - 用户显式可见终端或开发调试编排，例如 `pnpm dev`、集成 terminal。
  - 后台执行子进程，例如 relay command、stdio MCP、tool shell、workspace probe、function runner、runtime helper。
- 后台执行子进程必须通过统一进程启动边界创建；Windows 下该边界必须应用无窗口策略。
- HTTP/HTTPS MCP 仍保持纯网络 transport；如果它的操作伴随弹窗，必须通过进程启动诊断定位同一操作链中的其它 spawn 来源。
- 增加进程启动诊断，记录 domain/program/cwd/visibility，帮助从日志中定位真实弹窗来源。
- 扫描并收束当前 AgentDash 外围后台执行面中的裸 `Command::new` 和重复 `CREATE_NO_WINDOW`；Codex PTY 使用上游实现，不作为本任务修复面。
- 保持 `pnpm dev`、`scripts/dev-*` 等开发者编排可见、可观察，不纳入后台 hidden guard。

## Acceptance Criteria

- [ ] 有统一的后台进程启动边界，Windows 下应用无窗口策略。
- [ ] 后台执行路径使用统一边界或明确低层例外；新增 guard 能阻止未审计的裸后台 spawn。
- [ ] 进程启动 diagnostics 能说明 HTTPS MCP 这类伴随操作中真实启动子进程的来源。
- [ ] HTTP/HTTPS MCP transport 本身不新增任何进程启动语义。
- [ ] `pnpm dev` / `scripts/dev-*` 不被 hidden 策略影响。
- [ ] `cargo check -p agentdash-local` 通过。
- [ ] 更新 spec，记录后台执行 substrate 的静默原因，而不是记录过去错误实现。

## Out Of Scope

- 不在本任务实现 Windows Service 安装器或 service-first 产品形态。
- 不重构 runner enrollment、backend selection、relay auth。
- 不改变 MCP HTTP/HTTPS 协议语义。
- 不隐藏开发者显式启动的前台调试进程。
