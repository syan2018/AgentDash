# Windows 桌面安装包与后台运行

## Goal

将 Windows 桌面端交付为完整安装包，并提供后台运行、系统托盘、自启动和本机 Local Runtime 自动连接能力。

## Requirements

- 完整安装包继续基于 Tauri/NSIS，包含 Tauri 桌面壳、内置前端、Desktop API 与本机 runtime 管理能力。
- `Desktop API` 默认绑定 `127.0.0.1`，仅服务桌面内置前端。
- 桌面端提供系统托盘，菜单至少包含打开 AgentDash、启动/停止本机 runtime、查看状态、退出。
- 关闭窗口默认隐藏到托盘；显式退出才终止桌面进程。
- 设置项包含开机启动 AgentDash、启动后进入托盘、启动后自动连接本机 runtime。
- 正在执行任务时关闭窗口不应中断任务；显式退出需要有明确处理语义。
- 安装/卸载流程负责写入和清理自启动项。

## Acceptance Criteria

- [ ] 全新 Windows 安装后可启动桌面 UI 并连接云端。
- [ ] 关闭窗口后进程仍在后台运行，托盘可重新打开窗口。
- [ ] 显式退出后桌面进程退出，并按设计处理运行中的本机 runtime。
- [ ] 开机自启动后可按设置进入托盘并自动连接本机 runtime。
- [ ] Desktop API 只绑定 `127.0.0.1`。
- [ ] `pnpm run desktop:check` 与 `pnpm run desktop:bundle` 通过。

## Notes

- Windows 桌面自启动与独立 runner 的 Windows Service 是不同能力，不共享语义。
- 启动前补齐本子任务 `design.md` 与 `implement.md`。
