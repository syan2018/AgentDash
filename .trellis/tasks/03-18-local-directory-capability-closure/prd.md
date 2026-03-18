# 本机目录能力下沉与目录选择器收口

## Goal

把仍然滞留在 cloud 侧的本机目录能力收口到 local backend 或明确限制其调用边界，避免 cloud 正式路径继续直接操作宿主机目录。

## Background

当前边界重构已经把：

- Task 执行
- workspace 文件读写
- address space workspace file 搜索
- workspace detect git（创建/更新流程）

都收敛到了 `Workspace.backend_id` 对应的 local backend。

但仍有少量本机目录能力尚未完全下沉，例如：

- `pick_directory` 仍在 cloud 侧直接打开目录选择器
- 某些路径规范化/目录合法性校验仍默认发生在 cloud 宿主机

## Requirements

- 明确 `pick_directory` 的产品边界：要么迁移为 local backend 能力，要么明确只允许“本机 UI -> 本机 API”调用
- 进一步梳理目录存在性、路径规范化、目录合法性校验应放在哪一侧
- 保证 cloud 正式部署路径不需要访问宿主机目录选择器
- 对不适合进入 relay 的本机 UI 能力，给出明确架构限制和错误提示

## Acceptance Criteria

- [ ] `pick_directory` 不再作为 cloud 正式部署路径的隐式本地能力存在
- [ ] 目录相关 API 的部署语义稳定，不再依赖 cloud 当前跑在什么机器上
- [ ] 对调用边界有清晰文档和错误语义
- [ ] 若新增 relay 命令，`docs/relay-protocol.md` 同步更新

## Technical Notes

- 这个任务偏“能力边界收口”，不要求一次性把所有 UI 体验做满
- 如果最终决定 `pick_directory` 只允许本机调用，应把这一限制写进 API/产品约束而不是依赖默认假设
- 与 workspace source 解析任务分开推进，避免把“目录 UI 交互”与“上下文注入”耦合在一个实现里
