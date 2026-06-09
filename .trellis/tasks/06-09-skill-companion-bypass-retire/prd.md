# Child B — skill / companion 旁路退役收口

> Parent: [.trellis/tasks/06-09-capability-update-primitives](../06-09-capability-update-primitives/prd.md) · 模型见 parent design.md §4/§6(DC)。依赖 Child A 落地的 `AccumulationPolicy` 原语与 policy 词汇。

## Goal

退役 skill 与 companion 两条旁路，使其声明式真值与 tool 维度一样统一从 **base CapabilityState 投影**（policy=Replace），消除 frame_builder carry-forward 与 resolver 直接赋值的分叉。采用统一 base 投影方案，**不做高风险全量事件溯源改造**（决策 DC）。

## Requirements

1. **skill**：删除 frame_builder.rs L58-61 的 `skill.skills.is_empty() → inherit_skills_from` carry-forward；skill 真值统一经 resolver/base 投影写入 `CapabilityState.skill`，由 effective_capability_json 承载、按 revision 由 config 重新投影。
2. **companion**：确认 `capability/resolver.rs` 产出的 `companion.agents` 即 base 唯一来源，去除任何二次直接赋值/分叉路径；policy=Replace 落地。
3. 不引入新 effect 类型、不启用 companion 的 `set_agent_roster` effect 通道（保留给未来运行时增量）。
4. 与 Child 1 的 policy 归类对齐：skill=Replace、companion=Replace。

## Acceptance Criteria

- [ ] skill：set skill_asset_keys → 生效；clear → 回默认（不继承旧 skill）；新 agent → 默认。测试覆盖。
- [ ] companion：allowed_companions 变更经单一 base 投影生效，无旁路二次赋值；collaboration 能力门行为不变。
- [ ] frame_builder skill carry-forward 删除，grep 无残留。
- [ ] `cargo build --workspace` + `cargo test --workspace` 通过；**tool/vfs/mcp/workspace_module 零回归**（本 child 风险中，回归必须细致）。
- [ ] `pnpm contracts:check` 通过。

## Notes

- 本 child 触碰能力门投影，是全任务风险最高处；实现时优先补齐回归测试再改。
- 若实现中发现 companion 已无二次赋值（resolver 即唯一来源），companion 部分退化为"仅 policy 归类 + 注释"，据实记录。
