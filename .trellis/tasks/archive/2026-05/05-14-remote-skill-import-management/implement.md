# 实施计划：Pi Agent 优先的远端 Skill 导入与管理交互

## 阶段 1：补齐证据

1. 复核 `references/multica`。
   - 源码已拉取到 `references/multica`。
   - 后续只读检查，不改 reference 目录。

2. 复核 AgentDash skill 链路。
   - `crates/agentdash-domain/src/skill_asset/*`
   - `crates/agentdash-application/src/skill_asset/*`
   - `crates/agentdash-application/src/vfs/provider_skill_asset.rs`
   - `crates/agentdash-application/src/session/assembler.rs`
   - `crates/agentdash-application/src/skill/loader.rs`
   - `crates/agentdash-application/src/session/dimension/skill.rs`
   - 重点确认 Pi Agent / ProjectAgentLink 当前如何消费 `skill_asset_keys`，以及前端配置交互有哪些缺口。

3. 复核 multica 对照实现。
   - `server/internal/handler/skill.go`
   - `server/internal/handler/skill_create.go`
   - `packages/views/skills/components/create-skill-dialog.tsx`
   - `packages/views/skills/lib/origin.ts`
   - `packages/core/api/client.ts`
   - local/runtime skill 相关实现不作为主线。

## 阶段 2：实现 GitHub URL 导入

1. 后端应用层新增远端导入服务。
   - 解析 GitHub repo/tree/blob URL。
   - 下载 `SKILL.md` 与同目录支持文件。
   - 限制来源、路径、文件数量、单文件大小与总大小。
   - 解析 `SKILL.md` 元信息并创建 Project Skill Asset。
   - 保存来源 URL、origin type、导入时间和 digest。

2. API 新增导入入口。
   - Project 作用域下接收远端 URL。
   - 返回创建后的 Skill Asset 与来源信息。
   - 错误文案明确区分不支持 URL、缺失 `SKILL.md`、文件过大、路径非法、下载失败。

3. 前端补齐管理交互。
   - Skill Asset 面板提供 GitHub URL 导入入口。
   - 成功后刷新列表并选中新导入资产。
   - 资产列表或详情能展示来源。

## 阶段 3：Pi Agent 绑定检查

确认导入后的 Skill Asset 能被现有 Pi Agent `skill_asset_keys` 选择与注入，不增加 provider 本机目录依赖。

## 验证方式

- 本任务以源码审阅和设计一致性为主，不要求启动服务。
- 若后续进入实现前，需运行：
  - `pnpm run backend:check`
  - `cargo test -p agentdash-application skill`
  - 前端相关 typecheck / lint

## 暂不启动实现的条件

- 不为其它 provider/CLI 牺牲 Pi Agent 主路径。
- 不默认实现 multica local runtime skill system。
