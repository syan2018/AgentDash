# 执行计划

## Phase 1 Completion

- [x] 创建 Trellis 任务。
- [x] 编写 PRD。
- [x] 编写 design review 方案。
- [x] 配置 implement/check jsonl。

## Research Dispatch

- [x] 派发 Runtime surfaces research：D1 / D5 / D6 / D12。
- [x] 派发 Extension/action availability research：D7 / D8。
- [x] 派发 VFS/placement/local runtime research：D9 / D10 / D11。
- [x] 派发 Orchestration/gate/launch research：D2 / D3 / D4。

Dispatch prompt 固定要求：

- 以 `Active task: .trellis/tasks/06-30-design-backlog-review` 开头。
- research worker 读取 `implement.jsonl`、`prd.md`、`design.md`、`implement.md`、`design-backlog.md`。
- check worker 读取 `check.jsonl`、`prd.md`、`design.md`、`implement.md`、综合设计文档、research 和原始 `design-backlog.md`。
- 输出到本任务 `research/`，不改业务代码。
- 优先识别应删除或收束的旧路径，不以新增并行 abstraction 作为默认答案。
- 避免大规模 Rust 编译；设计 research 以代码证据和 targeted search 为主。

## Synthesis

- [x] 汇总 research，写 `design-review.md`。
- [x] 写 `decision-points.md`，只放需要用户选择的严肃设计点。
- [x] 写 `implementation-slices.md`，按依赖关系组织后续实现切片。
- [x] 用 `trellis-check` review 设计 package 的覆盖率和证据质量。

## Validation

- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-design-backlog-review`
- `git status --short`
- 文档链接和 referenced paths 可解析。
- 不运行 full test；本任务是 design review。
