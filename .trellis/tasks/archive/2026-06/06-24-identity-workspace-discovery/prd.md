# 按 Identity 策略发现本机 Workspace

## Goal

在 Project 设置页提供一套更直接的本机 Workspace 发现与绑定流程。系统按 Project 已有 Workspace identity 分派发现策略，在本机 backend 上查找可绑定目录；支持的策略返回候选，不支持的 identity 类型跳过；用户确认候选后写入 Workspace binding。

## User Value

- 用户不需要先理解 Backend Inventory / Workspace 详情 / 手动目录登记的完整链路。
- P4 项目可以利用已有 `p4_workspace` identity，在 Project 设置层快速发现并绑定本机 workspace root。
- 发现动作保持可解释：只展示候选，用户确认后才绑定，降低误绑风险。

## Confirmed Facts

- Workspace identity 已支持 `git_repo`、`p4_workspace`、`local_dir`。
- P4 identity 已支持 `server_stream`、`server_client`、`server_stream_client`、`path_key` 匹配模式。
- 本机 backend 已有给定目录后的 `workspace.detect` 能力，可识别 Git / P4 / Local facts。
- Project 设置页已有 `workspace` tab，当前包含 Backend Access、Workspace 列表和 Workspace Modules。
- 现有 Workspace 详情与创建抽屉支持手动浏览目录、detect、register inventory、创建或补 binding。

## Requirements

- 在 Project 设置页 `workspace` tab 增加 Project 级“本机 Workspace 发现”入口，位置在 Backend Access 与 Workspace 列表之间。
- 后端新增按 Workspace identity kind 分派的 discovery 策略模型。
- v1 只实现 `p4_workspace` 策略；未支持的 `git_repo`、`local_dir` 直接跳过，不作为错误。
- P4 策略按 Project Workspace 的 identity 在已授权本机 backend 上发现候选 root。
- 发现结果按 Workspace 分组展示，用户确认后才写入 binding。
- 绑定写入必须重新校验 Project 权限、backend access、候选 root 与 identity 匹配。
- 成功绑定后刷新 Workspace 列表、Backend Access inventory 与 workspace candidates。
- Workspace 详情保留手动维护能力，但 Project 设置页成为新增本机 workspace 的主入口。

## Acceptance Criteria

- [ ] Project 设置页可以触发本机 workspace discovery，且入口不在 Workspace 详情抽屉内。
- [ ] Unsupported identity kind 被记录为 skipped，不阻断 P4 discovery。
- [ ] P4 `server_stream` / `server_stream_client` identity 能返回本机候选 root。
- [ ] 多个候选只展示给用户消歧，不自动绑定。
- [ ] 用户确认候选后创建或更新对应 Workspace binding，并保持重复操作幂等。
- [ ] 绑定后相关列表刷新，用户能直接看到 Workspace 目录绑定状态变化。
- [ ] 离线 backend、未授权 backend、不可读 root、identity 不匹配都有可展示错误或 warning。

## Out Of Scope

- Git repository 自动发现策略。
- Local directory 自动发现策略。
- 静默自动绑定。
- 自动创建新的逻辑 Workspace。
