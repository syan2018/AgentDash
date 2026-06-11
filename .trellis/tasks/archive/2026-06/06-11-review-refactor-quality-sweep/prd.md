# PRD: 代码质量 review 与快速重构清理

## 背景

项目处于预研期，代码结构可以直接朝最正确的状态收敛，不需要保留兼容路径或回退方案。当前需要一个长期主控任务，通过工具扫描和 subagent 并行 review 暴露表面质量问题与架构设计风险，并按模块成组修复可控的实现级问题。

## 目标

- 创建并维护专用分支 `codex/review-refactor-quality-sweep`。
- 使用单个 Trellis 主任务跟踪全部 review、修复、验证和提交记录。
- 高并行地按模块发现问题、修复表面代码质量问题，并形成清晰的架构问题 backlog。
- 以模块为处理单元推进，避免为单个变量、单个 helper、单个小组件创建过碎的任务记录。

## 范围

本任务覆盖整个仓库，但每轮只处理边界清晰的模块或链路，例如：

- `settings-ui`
- `session-stream`
- `workflow-orchestration`
- `vfs-service`
- `local-runtime`
- `executor-connectors`
- `frontend-shared-contracts`
- `backend-application-boundaries`

## Review 目标

- 混淆命名、职责与名称不一致。
- 过于长程的裸字段传递。
- 重复、冗余、已接近架空但仍存在的链路。
- DTO / mapper / helper / 常量 / enum 的重复定义。
- frontend 组件、hook、store 职责过宽。
- backend 分层越界、反向依赖、application/domain/infra 耦合。
- frontend/backend 对同一业务语义重复解释。
- workflow/session/canvas/runtime/core 链路中的高耦合表现。
- 兼容性、回退性、旧实现适配路径等预研期不应保留的代码。

## 处理规则

- 架构设计问题只记录到 `architecture-backlog.md`，包含证据、影响面、建议方向和优先级，不在当前循环里贸然大改。
- 实现级质量问题按模块成组修复，并在 `fixes/` 下记录涉及文件、验证命令和 commit hash。
- 每个模块修复完成后由主控 review diff，运行与风险匹配的最小必要 check。
- 提交以模块为单位，格式遵守 `type(scope): 中文提交信息`，commit body 分点描述具体更新。
- 涉及数据库结构时必须处理 migration。
- 所有沟通、任务记录和总结使用中文。

## 交付物

- `review-index.md`：模块 review 总览、当前并行队列、已完成模块、提交索引。
- `architecture-backlog.md`：架构设计问题 backlog。
- `reviews/`：每轮模块 review 结论。
- `fixes/`：每个模块级修复记录。
- `tool-runs/`：`fuck-u-code`、lint、typecheck、测试等工具输出摘要。

## 验收标准

- 主任务目录包含上述跟踪文件和目录。
- 每轮 review 都能从 `review-index.md` 追踪到 review 记录、修复记录或 architecture backlog。
- 已修复模块有对应 commit，并记录验证命令和结果。
- 架构问题 backlog 中的条目具备代码证据、影响面、建议方向和优先级。
- 不引入兼容性回退路径，不保留明显旧实现适配链路。
