# 发布产物与验收流程 - Design

## Release Artifacts

第一阶段发布三类产物：

- Windows Desktop Installer：完整桌面安装包，包含 Tauri 壳、内置前端、Desktop API、Local Runtime 管理。
- Linux Local Runner：无 UI runner 二进制与 systemd service 安装说明。
- Windows Local Runner：无 UI runner 二进制与 Windows Service 安装说明。

每个产物都必须能输出版本信息，版本来自同一 release source。

## Validation Matrix

发布验收按产物组织：

- Windows Desktop：安装、启动、关闭到托盘、托盘恢复、自启动、自动连接 runtime、显式退出、卸载清理。
- Linux Runner：配置、service install/start/status/stop/uninstall、云端 online、断网重连。
- Windows Runner：配置、service install/start/status/stop/uninstall、云端 online、断网重连。

验收文档记录命令、预期结果、失败诊断入口，不依赖实现者口头知识。

## Version Contract

桌面壳、内置前端、Desktop API、Local Runtime/Runner、协议生成物必须来自同一构建版本。发布时不支持混合版本产物。

## Tradeoffs

- 第一版把发布验收固化成 checklist，自动化覆盖不足的系统安装行为用手工步骤补齐。
- 不要求用户使用开发脚本完成安装或服务托管。
