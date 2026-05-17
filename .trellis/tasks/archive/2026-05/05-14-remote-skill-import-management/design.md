# 设计草案：Pi Agent 优先的远端 Skill 导入与管理交互

## 产品取向

AgentDash 的一等 agent 是云端 Pi Agent。Skill 系统的主路径应围绕 Pi Agent、Project Skill Asset、VFS/session context 和 Context Inspector 的长期正确形态展开。

Claude/Codex/OpenCode/Cursor/Kiro 等 provider/CLI 可以作为退化版执行入口或迁移辅助，但不应反向牵引 AgentDash 的 Skill Asset 数据模型或 session 注入机制。multica 的 skill 系统整体不一定需要参考；最值得吸收的是“从远端 URL 下载 skill 并资产化”的能力。

## 当前 AgentDash 链路

### Skill Asset 资产层

- Domain：`crates/agentdash-domain/src/skill_asset/entity.rs`
- Application：`crates/agentdash-application/src/skill_asset/service.rs`
- API：`crates/agentdash-api/src/routes/skill_assets.rs`
- 前端：`frontend/src/services/skillAsset.ts`、`frontend/src/features/assets-panel/categories/SkillCategoryPanel.tsx`

Skill Asset 是 Project 级知识资产，文件有 `Skill` / `Reference` / `Script` / `Asset` 等分类，且支持 builtin seed 与用户上传。

### VFS 投影层

- `crates/agentdash-application/src/vfs/provider_skill_asset.rs` 将选中的 Skill Asset 投影为 `skills/<key>/...`。
- `crates/agentdash-application/src/vfs/mount.rs` 的 `append_skill_asset_projection` 根据 `agent_skill_asset_keys` 将 projection 追加到 session VFS。
- `crates/agentdash-application/src/skill/loader.rs` 可从 VFS 发现 `SKILL.md` 并生成 `SkillRef`。

当前链路已经具备“AgentDash session 可通过 VFS 与 context 看到 skill”的基础。Pi Agent 的 skill 使用体验应直接基于这些 AgentDash 原生资产能力优化。远端下载能力只负责把外部 skill 包导入为 Project Skill Asset，不改变 session 注入事实源。

### Session 层

- session assembler 会把 VFS、MCP、tool directives、skill asset keys 合成到 session request。
- skill dimension delta 会把 runtime skill 增删变更写入 ContextFrame。

## multica 参考点

`references/multica` 已拉取，当前已确认以下关键入口：

- `references/multica/server/internal/handler/skill.go`
- `references/multica/server/internal/handler/skill_create.go`
- `references/multica/packages/views/skills/components/create-skill-dialog.tsx`
- `references/multica/packages/views/skills/lib/origin.ts`
- `references/multica/packages/core/api/client.ts`

初步事实：

- `skill.go` 的 `ImportSkill` 支持 ClawHub、skills.sh、GitHub URL，根据 URL 判断来源并下载 `SKILL.md` 和支持文件。
- `fetchRawFile` 对单文件大小做上限控制，避免半下载内容静默进入 workspace。
- GitHub 导入支持 repo root、`tree/{ref}/{path}`、`blob/{ref}/{path}/SKILL.md` 等形态，并通过 GitHub API 收集支持文件。
- 导入时会将 provenance 写入 `config.origin`，前端通过 `origin.ts` 统一显示 `runtime_local`、`clawhub`、`skills_sh`、`github`、`manual` 来源。
- `create-skill-dialog.tsx` 支持 URL 输入、source 检测、导入状态文案和远端来源入口。

现有 `05-13` 研究与本轮修正后的可学习点：

- 远端导入是最值得学习的管理交互：用户输入 URL，系统下载并转成项目资产。
- 来源 metadata 和 source badge 有助于资产可解释性。
- GitHub URL 解析、支持文件收集、大小限制、路径校验值得学习。
- ClawHub/skills.sh 可作为扩展来源参考，但不必首版全部支持。
- local/runtime skill 体系可暂时丢弃，不进入主线。

## 本任务目标形态

AgentDash skill 系统应以“项目资产 + Pi Agent 显式绑定 + 可解释注入”为事实源，并补齐远端 skill 获取和管理交互：

1. 远端获取：用户输入 GitHub URL 或其它受支持来源 URL。
2. 解析预览：后端下载 `SKILL.md` 与支持文件，解析 name/description/disable_model_invocation，返回导入预览或直接导入。
3. 安全限制：限制来源、单文件大小、总大小、文件数量、路径 traversal、二进制内容策略。
4. 资产化：导入后写入 Project Skill Asset，记录 `origin = github`、source URL、repo/ref/path、digest。
5. Pi Agent 绑定体验：在 Pi Agent / ProjectAgentLink 配置中更容易选择、批量绑定、查看冲突和缺失 skill。
6. 注入执行：session 仍通过 AgentDash 既有 `agent_skill_asset_keys`、VFS projection、context bundle 进入运行时，不隐式读取用户本机 provider skill。
7. 诊断：Context Inspector / skill 管理 UI 能解释“这个 session 使用了哪些 skill，来自哪个项目资产，来源是什么”。

## 核心决策

### 事实源

Project Skill Asset 是 AgentDash 的事实源。Pi Agent 绑定的 `skill_asset_keys` 是 session 使用 skill 的主入口。provider 原生目录不作为 session 的隐式事实源。

### prompt / VFS / 管理 UI 职责

- prompt/context bundle 保留“有哪些 skill 可用、为什么可用、如何使用”的摘要。
- VFS projection 提供完整 `SKILL.md`、references、scripts 等内容。
- 管理 UI 展示来源、绑定、导入、冲突、缺失、诊断。

### 远端导入审计

导入记录或 Skill Asset metadata 至少需要表达：

- origin type：`github`、`manual`
- source URL、normalized URL、source host
- GitHub owner/repo/ref/path 或其它来源的 slug/id
- imported_at、imported_by
- file digest / bundle digest
- 原始 name/description 与导入后的 key/display_name 映射

### Pi Agent 优先级

所有交互首先服务 Pi Agent：更容易发现项目 skill、更安全地绑定 skill、更清楚地解释 session 注入结果。远端导入能力是辅助主线，因为它能把外部生态中的 skill 转成 Project Skill Asset；导入后仍应进入 Pi Agent 原生绑定与 VFS/session context 链路。

## 建议后续任务拆分

1. MVP：`feat(agent): Pi Agent skill 绑定与诊断体验`
   - 优化 Pi Agent 配置中的 Skill Asset 选择、批量绑定、缺失提示、来源展示。
   - Context Inspector 展示 Pi Agent session 实际注入的 skill。

2. 导入闭环：`feat(skill): GitHub URL Skill 导入`
   - 支持 GitHub repo/tree/blob URL。
   - 下载 `SKILL.md` 与支持文件，写入 Project Skill Asset。
   - 记录来源 metadata、digest、导入人和导入时间。

3. 绑定联动：`feat(agent): 导入后绑定到 Pi Agent`
   - Agent 配置页展示已绑定 skill、缺失 skill、来源、冲突。
   - 支持从导入结果直接绑定到 Pi Agent。

4. 诊断：`feat(session): session skill 来源与注入诊断`
   - Context Inspector 展示 session 实际使用的 Skill Asset、VFS projection 和来源信息。

5. 远端来源扩展：`feat(skill): Skills.sh / ClawHub Skill 导入`
   - 复核来源稳定性后决定是否支持。
   - 与 GitHub 导入共用验证、预览、来源 metadata。

## 风险

- 远端 skill 下载存在供应链风险，必须限制来源、大小、路径、文件数量，并清楚展示来源。
- 如果隐式读取本机 provider skill，会破坏 Project Skill Asset 的可解释性和团队一致性；因此本机来源若做也必须显式导入。
- 如果为了其它 provider/CLI 改变 Pi Agent 主路径，会让系统被退化版能力拖偏；因此 provider 适配只能在边缘层完成。
