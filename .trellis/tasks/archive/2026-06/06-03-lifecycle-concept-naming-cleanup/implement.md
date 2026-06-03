# Implementation Plan

## Checklist

- [ ] 启动任务并读取相关 backend/frontend/cross-layer spec。
- [x] 派 sub-agent 只读盘点前端命名混用点，写入 task research。
- [x] 更新 task PRD / design / implement。
- [x] 维护 backend workflow/session spec 的 Lifecycle 核心不变量。
- [x] 维护 frontend spec 的 definition/runtime/session view 边界。
- [x] 根据 research 结果做最多小范围、低风险前端命名或注释清理。
- [x] 整理后续概念清理计划。
- [x] 运行轻量检查：`git diff --check`，必要时运行相关前端 type/lint 或 targeted tests。

## Validation

- 文档变更：检查 `.trellis/spec` 内容是否只记录长期不变量、当前基线和稳定决策。
- 前端命名变更：用 `rg` 确认引用没有断裂。
- 最小命令：`git diff --check`。
- 若触及 TypeScript 代码导出或组件 props，运行对应 `pnpm` 前端检查或定向测试。

已执行：

- `pnpm -C packages/app-web exec tsc --noEmit`（implement sub-agent 执行，通过）
- `pnpm -C packages/app-web run lint`（主线程执行，通过；仍有 3 个既有 rounded warning，均在本轮目标外）
- `pnpm -C packages/app-web exec tsc --noEmit`（主线程执行，通过）
- `git diff --check`（主线程执行，通过）
- `rg -n "availableWorkflows|Workflow 来源|Workflow Key|引用 Workflow|Session 指引|Session 结束前|Session 终态|Latest Attempt|workflowDefs|wfMap|workflowForStep|workflowByKey|allWorkflowDefs|ProcedureKey" packages/app-web/src/features/workflow packages/app-web/src/features/task packages/app-web/src/features/story packages/app-web/src/pages/LifecyclePages.tsx`（主线程执行，无残留）
- `pnpm -C packages/app-web exec vitest run src/features/workflow/model/lifecycle-port-sync.test.ts src/features/workflow/ui/activity-inspector.test.tsx src/features/workflow/ui/panels/panels.test.tsx`（主线程执行，通过，22 tests）

## Risk Points

- `workflow` 在产品入口中仍有资产类目含义，不能粗暴全局改成 `lifecycle`。
- `RuntimeSession` 相关页面是用户会话视角，命名清理要区分用户语言和控制面语言。
- generated contract 和 route 名称暂不改，避免跨层合同漂移。
