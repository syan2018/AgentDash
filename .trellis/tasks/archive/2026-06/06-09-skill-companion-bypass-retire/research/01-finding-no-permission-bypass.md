# 结论：skill / companion 无权限旁路可退役（用户定调 + 探查证实）

## 用户定调
> skill 的权限不管动态发现的，只管平台授予的。

## 探查证实（skill）
- `CapabilityState.skill.skills: Vec<SkillEntry { name, description, file_path, disable_model_invocation }>` 是 `load_skills_from_vfs` 扫 VFS mount **物化出来的发现结果**（`crates/agentdash-application/src/skill/loader.rs:105`），即「动态发现」层。
- 平台授予 = config `skill_asset_keys` → `append_skill_asset_projection` 种进 VFS mount metadata（assembler.rs:599）→ 被扫描发现。**这条已是声明式 Replace**：每次 bootstrap 从 config 重投影；清空 keys → 不挂载 → 发现为空，**不复活旧 skill**（Explore Scenario A 证实）。
- `frame_builder` `inherit_skills_from`（L37 / L58-61）carry-forward 只在 lifecycle 热修订（不重扫 VFS）保住**已发现列表**，是发现层缓存，**非权限累积**。所有生产调用点已传 `None`（assembly_builder.rs:318），仅测试传 `Some`。盲删会在热修订丢 skill，且越界（不属能力权限原语）。

## 探查证实（companion）
- 唯一非测试 `.companion =` 写入在 `crates/agentdash-application/src/session/launch/command.rs:127`，是透传 resolver 产物，非第二源。
- `CapabilityState.companion.agents` 由 `CapabilityResolver`（resolver.rs:300-307）单源产出，受 `CAP_COLLABORATION` 门控。已是 Replace 单源。

## 决定（用户选「文档归类闭合，不动代码」）
- skill/companion 的**权限**本就声明式 canonical，**无旁路可退役**；唯一真正错配的是 workspace_module，已在 Child A 修复。
- 本 child 收敛为澄清 + 文档：在 docs/extension-system.md「能力更新原语」补「skill 归类边界」与「companion 单源」。policy 归类已在 Child A 落地（skill/companion=Replace）。
- carry-forward（发现缓存）原样保留，不动代码。

## 后续（可选，非本任务）
若要让 skill 的「授予」在 CapabilityState 上与「发现物化」显式分离（权限门承载 `skill_asset_keys`），属新增范围，另起任务。
