# Child B — skill / companion 归类闭合（无权限旁路可退役）

> Parent: [.trellis/tasks/06-09-capability-update-primitives](../06-09-capability-update-primitives/prd.md)。依赖 Child A 落地的 `AccumulationPolicy` 原语与 policy 词汇。
>
> **范围已根据用户定调与探查重定**（见 [research/01](research/01-finding-no-permission-bypass.md)）：原假设的"skill/companion 权限旁路"并不存在——两者权限本就声明式 canonical，唯一真正错配的 workspace_module 已在 Child A 修复。本 child 收敛为**澄清 + 文档归类闭合，不动代码**。

## 背景修正

- **skill 权限只管"平台授予"，不管"动态发现"**（用户定调）：授予 = config `skill_asset_keys`（声明式 Replace，清空即发现为空、不复活）；`CapabilityState.skill.skills`（`SkillEntry`）是 `load_skills_from_vfs` 的**发现物化结果**，非权限门。`frame_builder` 的 `inherit_skills_from` carry-forward 是**发现缓存**（热修订不重扫 VFS 时保住已发现列表），与能力权限原语无关 → 保留，不退役。
- **companion**：`CapabilityState.companion.agents` 由 `CapabilityResolver` 单源产出（受 `CAP_COLLABORATION` 门控），launch command 仅透传，无第二真值源 → 已 Replace 单源，无旁路。

## Goal

把"skill 权限=授予 / 发现是缓存"「companion=单源 Replace」这组归类认知落进文档，闭合 parent 的"6 维度归类"目标；不做代码改动（carry-forward 属发现层，按用户定调保留）。

## Requirements

1. docs/extension-system.md「能力更新原语」补「skill 归类边界」与「companion 单源」小节（已落）。
2. 记录探查结论与决策依据到 research/（已落）。
3. 确认 skill/companion 的 `policy()` 归类（=Replace）在 Child A 已落地，无需重复。

## Acceptance Criteria

- [x] 文档明确：skill 权限=`skill_asset_keys`(声明式 Replace)、`CapabilityState.skill` 是发现物化层非权限门、carry-forward 是发现缓存保留；companion=resolver 单源 Replace。
- [x] 探查结论与"无权限旁路"判定落盘 research/。
- [x] 不引入代码改动，故无构建/测试回归风险（沿用 Child A 已验证的绿状态）。

## Notes

- 这是忠于"拒绝表面修改"的结果：没有真旁路就不硬造一次代码改动来"完成任务"。
- 若将来要把 skill「授予」在 CapabilityState 上与「发现物化」显式分离，属新增范围，另起任务（research/01 末尾已记）。
