# 设计：本机执行 substrate 的窗口策略

## Core Diagnosis

Windows 弹窗来自 console 子进程窗口泄漏。根因不是 relay/MCP 类型，而是进程创建边界分散：

```text
GUI / desktop / background host
  -> 多个 crate 或外部库各自 spawn console 子进程
  -> 任一路径缺少无窗口创建策略
  -> 用户桌面出现 PowerShell/cmd/console 窗口
```

Codex Windows app 的公开手册说明 Windows native agent 使用 PowerShell 和 Windows sandbox，并提供 integrated terminal 作为 UI surface。实际排查中，用户直接使用 Codex 不会出现同类弹窗，因此本任务不把 Codex PTY / ConPTY 上游实现作为修复点；关键差异是 AgentDash 外围执行入口必须集中在受控 substrate 中，用户要看的 terminal 是 app 内的 terminal surface，不是随机 OS console window。

## Architecture

本任务的正确目标是建立 AgentDash 本机执行 substrate 的窗口策略，而不是引入新的 runner 宿主形态。

建议抽象：

- `ProcessVisibility`
  - `Background`：系统/远端/后台触发，Windows 下必须 hidden。
  - `UserVisible`：用户显式打开或开发者调试，允许可见。
- `ProcessDomain`
  - `mcp_stdio`
  - `tool_shell`
  - `terminal_pty`
  - `workspace_probe`
  - `function_runner`
  - `desktop_sidecar`
  - `codex_bridge`
  - `postgres_runtime`
- `background_std_command(domain, program)`
- `background_tokio_command(domain, program)`
- `apply_background_window_policy`

后台命令创建时记录 diagnostics：

```text
process_spawn domain=<domain> program=<program> cwd=<cwd?> visibility=background hidden_window=<bool>
```

## Boundaries

- `agentdash-local`：relay command、tool shell、MCP stdio、workspace probe、search、extension host 等本机执行入口。
- `agentdash-executor`：Codex bridge 等后台进程启动；不能手写重复 `CREATE_NO_WINDOW`。
- `agentdash-infrastructure`：function runner、postgres 管理命令；后台管理命令必须走统一策略。
- `codex-utils-pty`：使用上游 git 依赖，不作为 AgentDash 窗口策略修复面。
- `scripts/` 与 `pnpm dev`：开发者可见编排，排除在后台 hidden guard 之外。

## Guard

新增扫描脚本，只覆盖 AgentDash 外围后台 Rust 执行面：

- `crates/agentdash-local`
- `crates/agentdash-local-tauri`
- `crates/agentdash-executor`
- `crates/agentdash-infrastructure`
- `crates/agentdash-process`

检查裸：

- `Command::new`
- `tokio::process::Command::new`
- `std::process::Command::new`
- `creation_flags(CREATE_NO_WINDOW)`
- `CreateProcessW`

允许列表只保留：

- 统一 process substrate 模块。
- 测试文件。
- 明确标记为 `UserVisible` 或开发调试的入口。

## Non-goals

- Windows Service 不作为本任务主解；它可以是后续部署形态选择。
- 不把 Desktop/Dev/Service 拆成不同 runner 语义。
- 不把 HTTP/HTTPS MCP transport 误归因为进程启动来源。
