# 实施计划：AgentDash 与 multica 本地运行时概念对齐学习

## 执行步骤

1. 建立源码索引
   - AgentDash：`crates/agentdash-local`、`crates/agentdash-api/src/relay`、`crates/agentdash-relay`、`crates/agentdash-application/src/session`、`frontend/src/api`、`frontend/src/stores`。
   - multica：`server/internal/daemon`、`server/cmd/multica`、`server/internal/service/task.go`、`server/internal/handler/*runtime*`、`server/pkg/agent`、`server/pkg/protocol`、`packages/core/realtime`、`apps/desktop`。
   - 扩展到全量目录：AgentDash 所有 `crates/*`、`frontend/src/*`、`docs`、`scripts`；multica 的 `server/internal/*`、`server/pkg/*`、`server/migrations`、`apps/*`、`packages/*`、`docs`、`scripts`、Docker/self-hosting 文件。

2. 产出概念映射
   - 按项目/工作区/Agent/运行时/任务/会话/自动化/技能/通知/前端状态分组。
   - 每项标注 AgentDash 入口文件、multica 入口文件、相似点、关键差异、学习价值。

3. 产出目录映射
   - 按后端、应用服务、领域/数据层、协议、前端、桌面端、本机运行时、部署文档分组。
   - 先做到“后续要读某个概念时应该从哪里开始”的导航精度。

4. 深挖云端能力
   - 对比 API 路由、handler/service 分层、数据模型与迁移、事件总线、实时推送、权限/成员/工作区、通知/活动/评论。
   - 标注 multica 云端能力中 AgentDash 可以学习的产品闭环，而不是只看 daemon。

5. 深挖本地连接链路
   - 对比连接建立、注册、能力上报、心跳、重连、离线判断、任务下发、事件回传、取消、恢复。
   - 特别记录 multica 的 runtime gone、orphan task recovery、workspace GC、session pinning、task messages。

6. 深挖 desktop/local 一体化
   - 梳理 multica web/desktop/packages 分层。
   - 找出 desktop 是否复用 web views、如何接入本机 daemon/CLI、哪些 UI 暴露运行时管理。
   - 对照 AgentDash `04-28-tauri-desktop-unified-architecture`、`04-13-local-dashboard-ui` 等已有规划。

7. 形成学习 backlog
   - 按收益与风险排序。
   - 每个候选项给出建议拆分任务、涉及模块、验收口径。

## 建议研究文件

- `research/concept-map.md`
- `research/directory-map.md`
- `research/cloud-capability-map.md`
- `research/local-daemon-comparison.md`
- `research/desktop-local-integration.md`
- `research/learning-backlog.md`

## 验证方式

- 不运行产品服务也可完成；以源码与文档交叉验证为主。
- 需要时可运行只读统计命令，例如 `rg --files`、`rg -n`、`Get-Content`。
- 完成后检查研究文档是否满足 PRD Acceptance Criteria。

## 风险与注意

- 不要恢复旧的 `05-13-multica-reference-review` 删除目录，避免覆盖用户当前工作区状态。
- 不使用 subagent；本项目 AGENTS.md 已注明当前 Codex subagent 身份标识存在问题。
- 不把参考项目实现直接视为目标架构。所有学习项都必须说明如何适配 AgentDash 的 VFS、SessionHub、Hook Runtime、Lifecycle DAG。
