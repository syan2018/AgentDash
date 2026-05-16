# 远端 Skill 导入与管理交互

## Goal

在 AgentDash 现有 Project Skill Asset 架构上实现远端 skill 导入 MVP，并让导入后的 skill 更容易进入 Pi Agent 管理与绑定流程。multica 仅作为远端 URL 导入、来源识别、文件拉取限制和 UI 交互参考，不复刻其本机 runtime skill 体系。

## Confirmed Facts

- `references/multica` 已按 `references/repositories.json` 从 `https://github.com/multica-ai/multica` 拉取到本地，可直接进行逐文件复核。
- AgentDash 已有 Project 级 Skill Asset：`crates/agentdash-domain/src/skill_asset/*`、`crates/agentdash-application/src/skill_asset/*`、`crates/agentdash-api/src/routes/skill_assets.rs`。
- Skill Asset 可通过 `skill_asset_fs` VFS mount 投影为 `skills/<key>/...`，入口在 `crates/agentdash-application/src/vfs/provider_skill_asset.rs`。
- session 组装时会根据 `agent_skill_asset_keys` 追加 skill asset projection，入口在 `crates/agentdash-application/src/vfs/mount.rs` 与 `crates/agentdash-application/src/session/assembler.rs`。
- 当前 skill loader 可以从 VFS mount 或本地目录扫描 `SKILL.md`，入口在 `crates/agentdash-application/src/skill/loader.rs`。
- multica 的远端 skill 导入入口在 `references/multica/server/internal/handler/skill.go`：`ImportSkill` 支持 ClawHub、skills.sh、GitHub URL，下载 `SKILL.md` 与支持文件，创建 skill，并将来源写入 `config.origin`。
- multica 的来源展示入口在 `references/multica/packages/views/skills/lib/origin.ts`，支持 `runtime_local`、`clawhub`、`skills_sh`、`github`、`manual` 等来源类型。
- multica 的创建/导入 UI 在 `references/multica/packages/views/skills/components/create-skill-dialog.tsx`，包含 URL source 检测、导入状态文案和远端来源入口。
- multica 的 local/runtime skill 体系对 AgentDash 主线参考价值有限；本任务只保留其远端导入、来源记录、文件拉取限制与 UI 交互作为参考。
- 用户已明确产品取向：AgentDash 需要顾及的一等 agent 基本是云端 Pi Agent，其它 provider/CLI 可以视为附赠的退化版能力。Skill 系统设计应优先服务 Pi Agent、Project Skill Asset、VFS/session context 的长期正确形态。

## Requirements

- 复核 AgentDash skill 相关链路，明确 Skill Asset 从创建、绑定、VFS 投影到 session context 的完整数据流。
- 复核 multica 远端 skill 导入实现，重点对照 source detection、GitHub 下载、文件大小限制、支持文件收集、来源 metadata、UI 入口。
- 实现 AgentDash GitHub URL 导入 MVP：解析 repo/tree/blob URL，下载 `SKILL.md` 与支持文件，创建 Project Skill Asset。
- 在导入入口和资产展示中体现来源信息，让用户能判断 skill 来自手动创建、builtin seed 还是远端导入。
- 明确远端 skill 来源边界：首版优先支持 GitHub URL；ClawHub/skills.sh 仅保留为后续扩展候选。
- 明确哪些内容进入 prompt/context bundle，哪些内容通过 VFS 可读资源提供，哪些内容只作为管理 UI 的来源/诊断信息。
- 明确审计与可解释性：远端导入必须能追溯 source URL、source type、Skill Asset 与 digest。
- 明确安全边界：远端下载必须有大小限制、路径校验、支持文件数量限制、URL allowlist/来源识别、明确错误文案；导入后才成为 Project Skill Asset。
- 明确 Pi Agent 优先级：新增能力必须先回答“如何改善 Pi Agent 使用 Skill Asset 的体验”，再考虑其它 provider 的退化适配。
- 不实现本机 provider skill inventory/import；如后续仍有收益，再以独立任务评估。

## Acceptance Criteria

- [ ] 后端提供 Project Skill Asset GitHub URL 导入能力，下载 `SKILL.md` 与安全范围内的支持文件。
- [ ] 导入创建的 Skill Asset 保留来源类型、来源 URL、导入时间和 bundle digest。
- [ ] 前端 Skill Asset 管理入口支持输入 GitHub URL 导入，并能展示导入错误与来源信息。
- [ ] Pi Agent skill 绑定流程可以选择导入后的 Skill Asset，且不依赖 provider 本机目录。
- [ ] 覆盖 URL 解析、路径校验、下载限制、来源 metadata、API 契约与前端交互的测试或静态检查。

## Out of Scope

- 不把物理 provider 原生目录作为 AgentDash Skill Asset 的事实源；事实源仍是 AgentDash 项目资产与 VFS/session context。
- 不为其它 provider/CLI 牺牲 Pi Agent 主路径；其它 provider 只做退化适配或导入来源。
- 不默认实现 multica 的 local runtime skill inventory/import；如评估收益不足，可直接丢弃。
- 不默认支持所有远端来源；首版可以只支持 GitHub URL。
- 不直接引入 desktop local backend 控制台。
- 不把 Skill Asset 等同于 Plugin；Plugin API 不在本任务中重构。

## Notes

- 父任务：`05-13-multica-local-runtime-concept-alignment`。
- `references/multica` 已拉取，后续执行本任务时应以本地源码为准，已有 `05-13` 研究材料作为导航。
- 用户明确修正：AgentDash 一等 agent 是云端 Pi Agent，其它 provider/CLI 是附赠退化版；本任务应以 Pi Agent 的 Skill Asset 管理体验为主线。
- 用户明确修正：如果 multica skill 系统对 AgentDash 没必要参考，可以丢弃；但可以考虑支持从远端下载 skill 的 feature。
