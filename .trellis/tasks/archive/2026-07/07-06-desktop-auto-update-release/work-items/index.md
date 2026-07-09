# 主仓工作项索引

父任务保留为主仓通用能力的集成任务。私有对象存储部署适配已经拆到子任务 `07-06-desktop-update-private-deployment`。

主仓工作项按以下顺序逐个实现和 check：

1. [发布产物与 manifest 契约](./01-release-artifacts-manifest.md)
2. [云端 latest release / update endpoint](./02-cloud-update-endpoint.md)
3. [桌面 updater 流程](./03-desktop-updater-flow.md)
4. [强制更新阻断与只读诊断](./04-force-update-gate-diagnostics.md)

## Check Policy

- 每个工作项完成后先按本文件内的 `Checkpoints` 自查，再进入下一个工作项。
- 工作项可以共享同一父任务，不必再拆成更多 Trellis 子任务，除非实现时发现某项需要独立 PR 或独立验收。
- 私有仓部署适配不阻塞主仓代码实现，但主仓产物 schema 和 upload plan 必须足够稳定，能让私有仓消费。
