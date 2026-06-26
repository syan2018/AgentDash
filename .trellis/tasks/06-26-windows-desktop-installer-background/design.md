# Windows 桌面安装包与后台运行 - Design

## Architecture

Windows 桌面完整安装包由 Tauri 壳承载，包含内置前端、Desktop API 与 Local Runtime 管理能力。Desktop API 是桌面内置前端的 Dashboard API 宿主，默认绑定 `127.0.0.1`，不代表 Local Runner 的通信模型。

## Window And Tray Lifecycle

窗口生命周期：

- 点击关闭按钮默认隐藏到托盘。
- 托盘菜单 `打开 AgentDash` 重新显示主窗口。
- 托盘菜单 `退出 AgentDash` 才是真正退出桌面进程。

托盘菜单：

- 打开 AgentDash。
- 启动/停止本机 runtime。
- 查看当前运行状态。
- 退出 AgentDash。

正在执行任务时，关闭窗口不影响 Local Runtime；显式退出按设置页定义的退出策略处理运行中任务。

## Startup Settings

桌面设置包含：

- 开机启动 AgentDash。
- 启动后进入托盘。
- 启动后自动连接本机 runtime。

这些设置属于桌面 App 用户偏好，独立 runner 的 Windows Service 不复用这些语义。

## Packaging

安装包继续基于 Tauri/NSIS。安装负责注册应用、图标、卸载信息和可选自启动项；卸载负责清理安装期创建的自启动项。

## Tradeoffs

- 第一版只承诺 Windows 桌面完整安装包，避免 macOS/Linux 桌面生命周期差异扩大范围。
- 关闭到托盘保证桌面用户的任务不中断，显式退出保留用户对后台进程的控制权。
