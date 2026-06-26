# Windows 桌面安装包与后台运行 - Implement

## Checklist

- Tauri 桌面壳：
  - 增加系统托盘与菜单事件处理。
  - 拦截窗口关闭，默认隐藏到托盘。
  - 实现显式退出路径，退出时停止必要 sidecar 并处理 runtime 策略。
- 自启动：
  - 增加桌面设置持久化字段。
  - 实现 Windows 开机启动配置写入/清理。
  - 支持启动到托盘和启动后自动连接 runtime。
- Desktop API：
  - 验证默认绑定 `127.0.0.1`。
  - 设置页展示 Desktop API 状态。
- 安装包：
  - 完善 NSIS/Tauri bundle 元数据、图标、版本信息。
  - 验证安装、启动、卸载、自启动清理。

## Validation

- `pnpm run desktop:check`
- `pnpm run desktop:bundle`
- Windows 手工验收：
  - 安装后启动桌面 UI。
  - 关闭窗口后托盘可恢复。
  - 显式退出后进程退出。
  - 开机自启动按设置进入托盘并自动连接 runtime。

## Risk Points

- 关闭到托盘不能让用户误以为任务已经停止，状态入口必须清楚。
- 自启动项必须由安装/设置创建的路径清理，避免卸载后残留。
- Desktop API 不应绑定 `0.0.0.0`。
