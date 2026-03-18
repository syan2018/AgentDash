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
- 独立 `detect_git` API 在未提供 `backend_id` 时仍会 fallback 到 cloud 宿主机目录探测
- 某些路径规范化/目录合法性校验仍默认发生在 cloud 宿主机

进一步梳理后，第三条需要先澄清一个产品/架构事实：

- “浏览目录”首先是宿主机 UI 能力，而不是纯后端文件系统能力
- 如果把 `pick_directory` 直接做成 relay 命令，它选择到的是“backend 所在机器”的目录，不一定是“当前用户正在操作的这台机器”的目录
- 在 cloud、backend、浏览器三者可能分离的部署形态下，这会让路径语义比现在更混乱

因此本任务更稳妥的方向应当是：先收死 cloud 的隐式本机能力，再把“目录选择器”定义为未来的显式客户端本机能力，而不是急着把它塞进 relay。

## Requirements

- 明确 `pick_directory` 的产品边界：本轮不将其作为通用 relay/backend 能力引入
- cloud 正式部署路径中，`pick_directory` 不再隐式使用 cloud 宿主机 UI
- `detect_git` 等目录探测能力必须显式绑定 `backend_id`，不再悄悄 fallback 到 cloud 宿主机文件系统
- 进一步梳理目录存在性、路径规范化、目录合法性校验应放在哪一侧，并使其与 `Workspace.backend_id` 语义一致
- 保证 cloud 正式部署路径不需要访问宿主机目录选择器
- 对不适合进入 relay 的本机 UI 能力，给出明确架构限制、错误提示和后续产品化方向
- 前端在没有本机目录选择能力时，仍保留“手动输入 backend 侧绝对路径”的可用路径

## Acceptance Criteria

- [ ] `pick_directory` 不再作为 cloud 正式部署路径的隐式本地能力存在
- [ ] 未显式提供 `backend_id` 的目录探测请求不会再读取 cloud 宿主机目录
- [ ] 目录相关 API 的部署语义稳定，不再依赖 cloud 当前跑在什么机器上
- [ ] 对调用边界有清晰文档和错误语义，能区分“backend 路径”和“当前用户机器路径”
- [ ] 前端在无目录选择器能力时，仍能通过手动输入路径完成 workspace 创建/更新
- [ ] 若后续新增客户端本机目录选择能力，其接入路径与 backend relay 解耦，不混入 `docs/relay-protocol.md`

## Technical Notes

- 这个任务偏“能力边界收口”，不要求一次性把所有 UI 体验做满
- 本轮推荐方案：
  - 第一步，严格收口：`detect_git` 必须要求 `backend_id`；`pick_directory` 默认禁用并返回明确错误，或仅允许显式开发开关下使用
  - 第二步，前端配合收口：默认以“手动填写 backend 机器上的绝对路径”为主流程，并对“浏览目录”能力不可用给出明确文案
  - 第三步，若后续确实需要浏览目录 UX，再单独设计“客户端本机目录选择能力”，例如桌面壳层/本机桥接，而不是新增 relay 命令
- 如果保留开发便利性，可考虑增加显式环境开关，例如 `AGENTDASH_ENABLE_LOCAL_DIRECTORY_UI=1`，但默认必须关闭
- 与 workspace source 解析任务分开推进，避免把“目录 UI 交互”与“上下文注入”耦合在一个实现里

## Proposed Plan

### Phase 1: 边界收口

- `POST /workspaces/detect-git` 改为必须提供 `backend_id`
- cloud 侧删除或禁用基于本地 `std::fs` 的 fallback 探测逻辑
- `POST /workspaces/pick-directory` 默认返回明确错误，说明该能力不属于 cloud 正式部署路径
- 文档明确：`container_ref` 始终表示 `Workspace.backend_id` 所在机器上的路径

### Phase 2: UI 收口

- 前端把“浏览目录”从默认主路径降级为可选能力
- 在无能力时展示清晰提示，引导用户手动输入 backend 侧绝对路径
- 若保留开发态浏览目录，则通过显式 capability / 配置开关决定是否显示按钮，而不是默认调用

### Phase 3: 后续增强（单独任务）

- 如果产品确认需要“浏览我当前电脑上的目录”，应引入客户端本机桥接能力
- 如果产品确认需要“浏览 backend 机器上的目录”，应设计独立的 backend 文件浏览协议，但这不应伪装成当前用户本机目录选择器
