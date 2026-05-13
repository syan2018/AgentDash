# Research Index

本目录用于沉淀 AgentDash 与 `references/multica` 的分模块对比研究。

目标是先建立“概念与目录一一对应”的学习索引，再把值得学习的机制评估为后续正式 Trellis 任务。

## 已有资料

- `multica-module-review.md`：用户恢复的原始整体模块 review，作为第一版观察基线。
- `main-session-notes.md`：当前 Codex 主会话的研究记录、分工和待汇总项。
- `subagent-feature-synthesis.md`：四个只读 subagent 的多模块对比汇总，按可学习 feature、改造方式和优先级沉淀。
- `misc-insights.md`：未进入主线优先级、但未来可能启发设计的零散观察。

## 已知缺口

- 当前工作区中未发现 `multica-module-review.md`；如后续恢复，需要将其中观察与本目录现有产物交叉校验。

## 计划产物

- `concept-map.md`：AgentDash ↔ multica 核心概念映射。
- `directory-map.md`：两边目录职责映射与阅读入口。
- `cloud-capability-map.md`：云端 API、服务层、数据层、实时事件、权限/成员、通知/活动对齐。
- `local-daemon-comparison.md`：AgentDash local/relay 与 multica daemon/runtime 对比。
- `desktop-local-integration.md`：web/desktop/local 一体化体验对比。
- `learning-backlog.md`：可学习机制评估与后续正式任务候选。

## 分模块研究口径

每个专题研究尽量使用同一格式：

1. AgentDash 相关概念与目录。
2. multica 相关概念与目录。
3. 相似点与关键差异。
4. 值得学习的机制。
5. 不应直接照搬的机制。
6. 可转正式任务的候选项。

## 当前 subagent 分工

- `research_cloud_data_events`：云端服务、数据层、实时事件。
- `research_local_daemon`：本地连接、daemon/runtime、relay。
- `research_frontend_desktop`：前端包结构、状态管理、desktop/local 一体化。
- `research_product_automation`：产品协作模型、自动化、技能/知识资产。

这些 subagent 只读研究并回报结论；主会话负责最终写入本目录。
