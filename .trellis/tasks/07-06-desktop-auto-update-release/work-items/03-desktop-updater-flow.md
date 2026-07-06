# 工作项 3：桌面 updater 流程

## Goal

桌面端固定使用 stable channel，通过 AgentDash 云端 update endpoint 检查、下载、安装更新，并在下载完成后提示用户重启完成更新。

## Scope

- 接入 Tauri updater 插件和前端 bridge。
- 桌面端只调用 AgentDash 云端 API，不直接配置对象存储 latest URL。
- 支持启动后自动检查 stable 更新。
- 支持设置页手动检查更新。
- 展示当前版本、最新版本、更新状态、错误信息和重启入口。
- 非强制更新检查失败只在设置页/桌面更新区域/诊断日志展示，不打扰主界面。
- 不做静默重启。

## Deliverables

- Tauri updater 插件配置和权限。
- 桌面更新 bridge API。
- 设置页或桌面设置区的更新状态 UI。
- 更新状态机：idle、checking、up_to_date、available、downloading、installing、ready_to_restart、error。
- 非强制更新错误诊断展示。

## Checkpoints

- [ ] 桌面端更新检查只调用云端 update endpoint。
- [ ] 有新版本时能展示可更新状态。
- [ ] 能下载并安装签名正确的更新包。
- [ ] 下载或安装完成后只提示重启，不静默重启。
- [ ] 非强制更新失败不在主界面显示提醒。
- [ ] 缺少更新策略时不会阻断本地调试。
- [ ] 相关 UI 文案不把对象存储内部细节暴露给普通用户。

## Suggested Validation

- `pnpm run desktop:check`
- 桌面 bridge / UI 类型检查。
- 可用 fixture 或 mock endpoint 验证更新状态机。
