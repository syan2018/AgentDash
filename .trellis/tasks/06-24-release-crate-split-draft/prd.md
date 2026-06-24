# Release application crate split 主轴

## Goal

在 `codex/release-crate-split-refactor` 分支内，把 release 前 application crate split 从草案推进为可实施的全量重构主轴：同一 Trellis 任务维护需求、设计、实施顺序、工作项文件、subagent 派发和阶段提交记录。工作项保存在本任务目录内，原因是本次重构需要共享同一 import graph、同一分支和同一阶段提交节奏。

最终目标是让 `agentdash-application` 的内部 module graph 先表达正确依赖方向，再完成 application 相关 implementation crates 的物理拆分：

```text
agentdash-api / agentdash-local / agentdash-mcp
  -> application facade crates
  -> agentdash-application-ports
  -> agentdash-domain / agentdash-spi / protocol/type crates
```

## Context

- 父任务：`.trellis/tasks/06-24-release-crate-boundary-review`
- 当前承载分支：`codex/release-crate-split-refactor`
- 前置实施任务：`.trellis/tasks/06-24-agentrun-runtime-session-decoupling`
- 本项目尚未上线，重构以正确边界为准，module/API 形态直接收束到目标模型。
- 允许在此分支内做阶段性提交；阶段提交以边界锚定或机械迁移为检查点，最终集成收敛以 workspace-level gate 为准。
- 高并发重构以文件所有权隔离为第一约束；implement agents 优先使用 `rg`、批量 move、批量 import rewrite、`cargo metadata`、精确 `cargo check -p` 和可控 `cargo fix` 推动机械迁移，减少逐行手工 import 修补。
- implement agents 只运行自己 work item 的最小 gate；大测试、波次 readiness 和架构一致性判断由 check agents 在 checkpoint 统一收口。
- 冗余路径、重复 facade、错误链路、旧命名兼容壳，以及只被业务无关 test 锚定的陈旧行为，按目标架构优先删除路径和对应 test，再由 check agent 确认删除是否符合事实源。

## Current Baseline

- `AgentFrameRuntimeTarget` 已在 `agent_run/runtime_target.rs`。
- 旧 API helper `session_construction.rs` 已不存在；当前为 `agent_run_runtime_surface.rs`。
- `agentdash-application-ports::runtime_gateway_mcp_surface` 已存在。
- RuntimeGateway MCP access 已消费 ports crate，不再依赖 AgentRun implementation。
- `AgentRunRuntimeSurfaceQuery` 已区分 `launch_evidence_frame_id` 与 `current_surface_frame_id`。
- `AgentRunResourceSurfaceQuery` 已存在，API 的 runtime/AgentRun VFS source 不再 route-local 选择 latest anchor。
- Canvas / Extension runtime path Project guard 已部分落地。
- accepted launch 的 AgentFrame / Lifecycle writes 已迁到 AgentRun launch commit adapter。
- 当前硬阻塞仍是 `session <-> agent_run`、`agent_run <-> lifecycle`、RuntimeGateway setup action implementation imports、RuntimeSession live hub business deps、API/helper implementation DTO imports、VFS owner-specific provider deps。

## Requirements

- 以本任务为唯一主轴维护完整执行计划；工作项使用 `work-items/*.md` 文件，原因是所有工作项共享同一分支、同一边界图和同一验证矩阵。
- 每个工作项必须声明目标、可并行边界、拥有的路径、依赖的 port/facade、批量替换策略、验收命令和阶段提交建议。
- 优先使用批量移动、批量 import rewrite、manifest-level crate addition、static grep gate 等机械策略，减少逐点手工修补。
- 先锚定目标 crate / port / facade 关系，再修复编译错误；允许先搬文件到新 crates 造成暂时红灯，再按工作项收敛。
- 新 ports 保持纯 DTO/trait/error，原因是它们要被 API/local/runtime composition root 与 implementation crates 双向消费，同时保持 implementation 可替换。
- API route/helper 只保留 auth、DTO、path parsing、error mapping；current/resource/runtime surface 与 runtime placement 通过 application facade/port 消费。
- RuntimeGateway 只拥有 action registry、actor/context admission、provider dispatch、input/output validation；MCP/setup/extension backing facts 通过 ports 注入。
- RuntimeSession 只拥有 delivery、trace、turn/event stream、connector continuation、runtime registry、active turn/live adoption mechanics；业务 surface、AgentFrame write、Lifecycle reducer、Permission/Canvas/WorkspaceModule 归属各自控制面，原因是这些事实源需要从 AgentRun/Lifecycle 可审计地回放。
- AgentRun owns current/runtime/resource surface query、effective capability/admission、frame construction/update、surface update/adoption command boundary、mailbox/workspace command surface。
- Lifecycle owns LifecycleRun control ledger、subject association、orchestration activation/reducer/scheduler/materialization、RuntimeSessionExecutionAnchor 写入与 terminal callback 到 reducer。
- VFS core 抽取晚于 AgentRun resource surface 和 owner-specific provider 边界收束。
- 每个 wave checkpoint 必须派 check agents 检查 boundary purity、import graph、dead path/test deletion 和下一 wave readiness，原因是高并发 implement 的局部验证不足以证明整体边界已经可移动。

## Work Item Files

| File | Purpose |
| --- | --- |
| `work-items/00-dispatch-map.md` | 总派发表、并行 lane、冲突规则、阶段提交约定。 |
| `work-items/01-ports-boundary-expansion.md` | 扩展 `agentdash-application-ports` 的纯 port/DTO/error。 |
| `work-items/02-runtime-gateway-setup-boundary.md` | RuntimeGateway setup actions port 化并准备 RuntimeGateway crate extraction。 |
| `work-items/03-agentrun-surface-facade.md` | AgentRun current/resource surface、effective capability、terminal/runtime placement facade。 |
| `work-items/04-runtime-session-substrate-boundary.md` | RuntimeSession facade、launch/live hub/adoption/mailbox 依赖倒置。 |
| `work-items/05-agentrun-lifecycle-boundary.md` | AgentRun/Lifecycle 之间的 projection/materialization/session creation ports。 |
| `work-items/06-api-consumer-facade-cleanup.md` | API route/helper current surface、VFS、terminal、extension、trace view helper 收束。 |
| `work-items/07-vfs-resource-surface-boundary.md` | AgentRun resource surface 与 generic VFS core / owner providers 分界。 |
| `work-items/08-public-visibility-cleanup.md` | application root、session、agent_run、lifecycle、vfs public facade 收缩。 |
| `work-items/09-physical-crate-extraction-runtime.md` | RuntimeGateway / RuntimeSession implementation crates 抽取。 |
| `work-items/10-physical-crate-extraction-control-plane-vfs.md` | AgentRun / Lifecycle / VFS core 后续物理抽取。 |

## Acceptance Criteria

- [x] 完整复读 parent/child 任务文件、research、review briefs、implement/check manifests 和相关 backend specs。
- [x] 创建并切换到承载分支 `codex/release-crate-split-refactor`。
- [x] 使用 `task.py set-branch` 把本任务绑定到承载分支。
- [x] 本任务进入 `in_progress`，后续工作全部归档在同一任务下。
- [x] `prd.md`、`design.md`、`implement.md` 反映“分支内全量重构主轴”。
- [x] `work-items/*.md` 覆盖全部可并行工作项，并能直接作为 subagent 派发输入。
- [x] `implement.jsonl`、`check.jsonl` 注入主轴文件、work item、research 与关键 spec。
- [x] Wave 1 ports-only 工作项完成并至少通过 `cargo check -p agentdash-application-ports`。
- [ ] Wave 2 import cleanup / facade contraction 完成，static grep gates 显示核心双向引用已改为 port/facade。
- [ ] RuntimeGateway 与 RuntimeSession crates 完成物理抽取，并通过对应 crate check。
- [ ] AgentRun 与 Lifecycle crates 完成物理抽取，并通过对应 crate check 或明确剩余 compile blockers。
- [ ] VFS core extraction 完成或留下经 design 批准的延后边界，owner-specific provider 依赖方向清楚。
- [ ] 最终集成记录完整 validation 状态、未绿测试和下一步修复点。

## Notes

本任务现在是 execution mainline。阶段性 red commit 的价值在于快速固定 crate/module 边界，让编译器暴露剩余引用关系；每个阶段提交需要说明当前完成的边界和下一步收敛目标。
