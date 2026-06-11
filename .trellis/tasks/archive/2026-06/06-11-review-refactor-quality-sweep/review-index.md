# Review Index

## 状态

- 分支：`codex/review-refactor-quality-sweep`
- 当前阶段：存量快速修复完成，架构后续已转入 `06-11-architecture-backlog-followup`，准备归档
- 主控：`codex-agent`

## 当前并行队列

| 模块 | 类型 | 状态 | 记录 |
| --- | --- | --- | --- |
| settings-ui | 预备修复 | 已提交 | `fixes/000-settings-ui-system-sections.md` |
| vfs-service | review | 已归档 | `reviews/001-vfs-service.md` |
| workflow-orchestration | review | 已归档 | `reviews/002-workflow-orchestration.md` |
| session-stream | review | 已归档 | `reviews/003-session-stream.md` |
| local-runtime | review | 已归档 | `reviews/004-local-runtime.md` |
| settings-llm-providers | review | 已归档 | `reviews/005-settings-llm-providers.md` |
| workflow-orchestration | 修复 | 已提交 | `fixes/001-workflow-orchestration-quick-cleanup.md` |
| session-stream | research | 已归档 | `research/session-stream-executable-plan.md` |
| local-runtime | research | 已归档 | `research/local-runtime-executable-plan.md` |
| settings-llm-providers | 修复 | 已提交 | `fixes/003-settings-llm-providers-model-boundary.md` |
| local-runtime Batch A | 修复 | 已提交 | `fixes/004-local-runtime-tool-executor-boundary.md` |
| local-runtime Batch B | 修复 | 已提交 | `fixes/005-local-runtime-tool-error-mapping.md` |
| local-runtime Batch C | 修复 | 已提交 | `fixes/006-local-runtime-relay-mcp-fail-closed.md` |
| local-runtime Batch D | 修复 | 已提交 | `fixes/007-local-runtime-extension-host-api-split.md` |
| vfs-service | 修复 | 已提交 | `fixes/002-vfs-create-text-error-semantics.md` |
| session-stream Batch 1 | 修复 | 已提交 | `fixes/008-session-stream-core-policy-cleanup.md` |
| session-stream Batch 2 | 修复 | 已提交 | `fixes/008-session-stream-core-policy-cleanup.md` |
| vfs-service | research | 已归档 | `research/vfs-service-executable-plan.md` |
| session-stream Batch 3 | 修复 | 已提交 | `fixes/009-session-context-frame-ui-view-model.md` |
| vfs-service Batch D | 修复 | 已提交 | `fixes/010-vfs-tool-boundary-cleanup.md` |
| vfs-service Batch E | 修复 | 已提交 | `fixes/010-vfs-tool-boundary-cleanup.md` |
| vfs-service Batch A | 修复 | 已提交 | `fixes/011-vfs-search-identity-propagation.md` |
| vfs-service Batch C | 修复 | 已提交 | `fixes/012-vfs-runtime-metadata-accessors.md` |
| vfs-service Batch B | 修复 | 已提交 | `fixes/013-vfs-patch-path-target-helper.md` |
| vfs-service Batch F | 修复 | 已提交 | `fixes/014-vfs-search-service-split.md` |
| workflow-orchestration | follow-up research | 已归档 | `research/workflow-orchestration-executable-plan.md` |
| local-runtime | follow-up research | 已归档 | `research/local-runtime-followup-executable-plan.md` |
| workflow-orchestration Batch A | 修复 | 已提交 | `fixes/015-workflow-script-preflight-convergence.md` |
| local-runtime Batch E | 修复 | 已提交 | `fixes/016-local-runtime-mcp-prompt-wire-shape.md` |
| local-runtime Batch G | 修复 | 已提交 | `fixes/017-local-runtime-host-api-fallback-removal.md` |
| workflow-orchestration Batch B | 修复 | 已提交 | `fixes/018-workflow-root-args-activation-input.md` |
| local-runtime Batch F | 修复 | 已提交 | `fixes/019-local-runtime-process-executor.md` |
| local-runtime Batch H | 修复 | 已提交 | `fixes/020-local-runtime-search-executor.md` |
| workflow-orchestration Batch C | 修复 | 已提交 | `fixes/021-workflow-ready-node-coordinate.md` |
| workflow-orchestration Batch D | 修复 | 已提交 | `fixes/022-workflow-executor-launcher-split.md` |
| companion-tools | research | 已归档 | `research/companion-tools-executable-plan.md` |
| companion-tools Batch 1 | 修复 | 已提交 | `fixes/023-companion-tool-context.md` |
| companion-tools Batch 2 | 修复 | 已提交 | `fixes/024-companion-sub-dispatch-launch.md` |
| companion-tools Batch 4 | 修复 | 已提交 | `fixes/025-companion-platform-grant.md` |
| frontend-canvas-workflow | research | 已拆分 | `research/canvas-runtime-preview-executable-plan.md`; `research/workflow-binding-panels-executable-plan.md` |
| executor-connectors | research | 已拆分 | `research/executor-connector-bridges-executable-plan.md` |
| canvas-runtime-preview | research | 已归档 | `research/canvas-runtime-preview-executable-plan.md` |
| workflow-binding-panels | research | 已归档 | `research/workflow-binding-panels-executable-plan.md` |
| executor-connector-bridges | research | 已归档 | `research/executor-connector-bridges-executable-plan.md` |
| session-ui-toolcards | research | 已归档 | `research/session-ui-toolcards-executable-plan.md` |
| mcp-preset-connectors | research | 已归档 | `research/mcp-preset-connectors-executable-plan.md` |
| session-ui-toolcards Batch A | 修复 | 已提交 | `fixes/026-session-capability-model.md` |
| mcp-preset-connectors Batch A | 修复 | 已提交 | `fixes/027-mcp-preset-form-helper.md` |
| session-ui-toolcards Batch B | 修复 | 已提交 | `fixes/028-session-companion-request-view-model.md` |
| mcp-preset-connectors Batch B | 修复 | 已提交 | `fixes/029-mcp-probe-view-model.md` |
| workflow-orchestration clippy gate | 修复 | 已提交 | `fixes/030-workflow-launch-outcome-clippy.md` |
| workflow-binding-panels Batch A | 修复 | 已提交 | `fixes/031-workflow-binding-panels-compact.md` |
| executor-connector-bridges Batch A/B | 修复 | 已提交 | `fixes/032-executor-mcp-adapter-boundary.md` |
| canvas-runtime-preview Batch C | 修复 | 已提交 | `fixes/033-canvas-runtime-panel-naming.md` |

## 已完成模块

| 模块 | commit | 验证 |
| --- | --- | --- |
| settings-ui | `9c7999a0` | `pnpm --filter app-web run typecheck`; `pnpm --filter app-web run lint`; `fuck-u-code analyze packages/app-web/src/features/settings/ui ...` |

## 待 review 候选

| 模块 | 来源 | 备注 |
| --- | --- | --- |
| vfs-service | `fuck-u-code` 初筛 | `crates/agentdash-application/src/vfs/service.rs` |
| workflow-orchestration | `fuck-u-code` 初筛 | `runtime.rs` / `compiler.rs` / `script_compiler.rs` / `executor_launcher.rs` |
| local-runtime | `fuck-u-code` 初筛 | `crates/agentdash-local/src/tool_executor.rs` |
| session-stream | `fuck-u-code` 初筛 | `packages/app-web/src/features/session/model/useSessionStream.ts` |
| settings-llm-providers | `fuck-u-code` 初筛 | `LlmProvidersSection.tsx` 文件过大 |

## 模块级 refactor 候选

这些项体现耦合或职责过宽，但未达到 architecture backlog 门槛，应优先按模块快速修复。

| 模块 | 来源 | 候选 |
| --- | --- | --- |
| vfs-service | `reviews/001-vfs-service.md` | runtime tool provider composition root 位置不当，可先在模块内重命名/拆出 VFS tool factory |
| vfs-service | `reviews/001-vfs-service.md` | API resolver 编排过重，可先收敛 helper/adapter 边界 |
| workflow-orchestration | `reviews/002-workflow-orchestration.md` | `OrchestrationExecutorLauncher` 过宽，可按 executor kind 渐进拆小服务 |
| workflow-orchestration | `reviews/002-workflow-orchestration.md` | `ReadyNodeTarget` 裸传坐标和快照，可先引入 typed coordinate/view |
| session-stream | `reviews/003-session-stream.md` | `useSessionStream` 暴露职责过宽，可先拆 event stream/feed/control 边界 |
| session-stream | `reviews/003-session-stream.md` | platform/session_meta_update 策略分散，可先抽 `sessionEventPolicy` |
| local-runtime | `reviews/004-local-runtime.md` | `CommandHandler` 过宽，可拆 command router 和领域 handler context |
| local-runtime | `reviews/004-local-runtime.md` | `ToolExecutor` 过宽，可拆 workspace boundary、file/search/shell executor |
| local-runtime | `reviews/004-local-runtime.md` | prompt MCP servers raw JSON，可收敛 relay/local typed contract |
| local-runtime | `reviews/004-local-runtime.md` | Extension Host 未接通 API surface 可接通或移除 |
| local-runtime | `reviews/004-local-runtime.md` | env/process 权限边界可收敛为 process/env policy |
| local-runtime | `reviews/004-local-runtime.md` | 搜索 rg/fallback 双链路可统一 contract 或删除 fallback |
| settings-llm-providers | `reviews/005-settings-llm-providers.md` | `LlmProviderForm` 可拆状态 hook 和小组件 |
| settings-llm-providers | `reviews/005-settings-llm-providers.md` | models/blocked_models parse/serialize 可移入 model 并测试 |
| settings-llm-providers | `reviews/005-settings-llm-providers.md` | probe/OAuth action 可从 UI 下沉到 model |
| settings-llm-providers | `reviews/005-settings-llm-providers.md` | provider preset 可先移入 settings model 常量 |

## 提交索引

| commit | 模块 | 摘要 |
| --- | --- | --- |
| `9c7999a0` | settings-ui | 收敛系统设置区块组件结构 |
| `c079e519` | workflow-orchestration | 清理不可达诊断、未用编译模式与误导工具描述 |
| `b9095329` | vfs-service | 收敛 `create_text` 错误语义 |
| `205d2a91` | settings-llm-providers | 收敛 provider 模型解析、preset 与 action 边界 |
| `f9f53388` | local-runtime | 收敛 ToolExecutor、ToolError、Relay MCP 与 Extension Host API 边界 |
| `4173fbcf` | session-stream | 收敛 stream reducer、transport contract、event policy 与 feed mapper |
| `5dc5cdc1` | session-stream | 收敛 context frame UI 输入为解析后 view model |
| `a08122a3` | vfs-service | 收敛工具路径 normalize 与 VFS tool factory 装配 |
| `ed7852b4` | vfs-service | 传递 search/grep identity 到 provider 与 inline grep |
| `46cbdbfb` | vfs-service | 收敛 runtime file metadata 常量与访问器 |
| `c2390ff7` | vfs-service | 统一 patch path target 解析与 mutation key 语义 |
| `264ec228` | vfs-service | 拆出 search/grep 专属服务边界 |
| `2bbb515c` | local-runtime | 删除 runtime.invoke 与 extension.channel_invoke host api 占位回退 |
| `1ac95573` | local-runtime | 对齐 prompt MCP relay 发送形态 |
| `37be46fa` | workflow-orchestration | 收敛 workflow script capability summary 单一解释器 |
| `bc29c1b6` | workflow-orchestration | 收敛脚本根参数 typed activation input |
| `2bff89ac` | local-runtime | 收敛 shell/exec 共享进程执行边界 |
| `9cfeaff1` | workflow-orchestration | 收敛 ready node typed coordinate/view |
| `fe41d9ed` | local-runtime | 拆出 SearchExecutor 与 FileDiscoveryPolicy |
| `41e01337` | workflow-orchestration | 拆分 executor launcher 按 executor kind 的服务边界 |
| `d64f5fcf` | companion-tools | 收敛 companion 工具 runtime context 与 session services 错误边界 |
| `f3601887` | companion-tools | 闭合子协作 control-plane 到 child runtime turn 启动链路 |
| `709ac9a7` | companion-tools | 闭合平台授权假请求链路 |
| `cb9cb9ab` | session-ui-toolcards | 收敛 capability 资源解析边界 |
| `f797c541` | mcp-preset-connectors | 收敛前端表单 helper |
| `d43ce563` | session-ui-toolcards | 收敛 companion 请求视图模型 |
| `cdd47f46` | mcp-preset-connectors | 收敛 probe 展示模型 |
| `9be19743` | workflow-orchestration | 修复编排启动结果 large enum clippy 阻塞 |
| `b0df9ce4` | workflow-binding-panels | 清理 binding panels deprecated compact 链路 |
| `bc949430` | executor-connector-bridges | 收敛 MCP adapter 共用核心与 naming 归位 |
| `e2ac5d35` | canvas-runtime-preview | 将 Canvas runtime 面板从 SessionPanel 命名收窄为 RuntimePanel |
