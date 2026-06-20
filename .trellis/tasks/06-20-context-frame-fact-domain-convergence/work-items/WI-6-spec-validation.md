# WI-6 Spec、测试与最终集成验收

## Status

planned

## Goal

把最终事实域契约沉淀到项目规范，并完成跨层验证。

## Scope

- 更新 backend capability spec。
- 更新 cross-layer ContextFrame / session context spec。
- 同步前端 ContextFrame 展示规范。
- 执行后端 targeted tests、前端 check、必要的 broader backend tests。
- 记录已知无关失败和剩余风险。

## Primary Files

- `.trellis/spec/backend/capability/*.md`
- `.trellis/spec/cross-layer/**/*.md`
- `.trellis/spec/frontend/**/*.md`
- `.trellis/tasks/06-20-context-frame-fact-domain-convergence/*.md`

## Acceptance

- [ ] spec 记录最终事实域契约。
- [ ] targeted tests 覆盖关键数据流。
- [ ] frontend check 通过。
- [ ] backend broader test 通过，或无关失败有具体测试名和现象。
- [ ] `work-items.md` 中所有 WI 状态完成并记录验证结果。

