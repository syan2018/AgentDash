# 发布产物与验收流程 - Implement

## Checklist

- 发布产物：
  - 产出 Windows Desktop Installer。
  - 产出 Linux Local Runner 二进制与 service 安装说明。
  - 产出 Windows Local Runner 二进制与 service 安装说明。
- 版本：
  - 桌面安装包、runner 二进制、协议生成物记录同一版本号。
  - 每个产物支持版本输出或安装包版本查看。
- 验收文档：
  - 编写 Windows Desktop 手工验收 checklist。
  - 编写 Linux Runner 手工验收 checklist。
  - 编写 Windows Runner 手工验收 checklist。
  - 每项包含命令、预期结果、失败诊断入口。
- 收口：
  - 确认卸载清理自启动项、服务项和安装期创建的文件。
  - 确认断网重连与云端 online/offline 状态变化。

## Validation

- `pnpm run desktop:bundle`
- Runner release build 命令在子任务实现后固定。
- Windows Desktop、Linux Runner、Windows Runner 三套手工验收全部通过。

## Risk Points

- 发布产物不能混用不同版本。
- 手工验收步骤必须能由非实现者执行。
- 卸载清理不能删除用户工作区或任务产物。
