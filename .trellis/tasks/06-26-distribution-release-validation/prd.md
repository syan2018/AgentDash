# 发布产物与验收流程

## Goal

定义 Windows 桌面安装包、Linux runner、Windows runner 的发布产物、版本一致性要求和手工验收流程，保证交付形态能被真实用户安装和托管。

## Requirements

- 明确发布产物：Windows 桌面完整安装包、Linux runner 二进制/服务安装说明、Windows runner 二进制/服务安装说明。
- 桌面壳、内置前端、Desktop API、Local Runtime/Runner、协议契约需要有一致版本标识。
- 发布验收覆盖安装、启动、后台运行、自启动、服务安装、服务卸载、断网重连、卸载清理。
- 验收文档记录每个产物的命令、预期结果和失败诊断入口。
- 发布流程不得要求用户手动修改源代码或开发脚本。

## Acceptance Criteria

- [ ] Windows 桌面安装包可安装、启动、后台运行、自启动、卸载。
- [ ] Linux runner 可安装为 systemd service，启动后云端显示 online。
- [ ] Windows runner 可安装为 Windows Service，启动后云端显示 online。
- [ ] 三类产物都能展示或输出版本信息。
- [ ] 验收 checklist 能由非实现者按步骤执行。

## Notes

- 本任务是集成收口任务，依赖其他子任务完成可验证产物。
- 启动前补齐本子任务 `design.md` 与 `implement.md`。
