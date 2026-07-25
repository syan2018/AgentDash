# Implement · Agent Runtime 与 Workspace/Channel 集成

本文件是规划阶段执行草案。Git checkout/worktree 拓扑可按用户要求预先准备；只有用户完成最终规划
审阅后才激活任务和修改产品代码。

## Phase 0 · Planning Handoff And Worktree

- 完成 PRD convergence、design review、implement/check JSONL。
- 将当前未提交规划目录连同 untracked files 暂存。
- 在主工作目录创建并切换 integration branch：

      git switch -c codex/agent-runtime-workspace-integration efdfa5dc

- 将原分支放入只读 source worktree：

      git worktree add F:\Projects\AgentDash-workspace-duplex-source codex/workspace-duplex-interaction-planning

- 在主工作目录恢复规划目录，验证 task status 仍为 planning。
- 最终审阅通过后，在 integration branch 提交规划并执行 task.py start。
- 验证 PR #93 baseline：fmt、workspace check、Runtime focused tests、contracts check。
- 以 95 个重叠路径建立 coupling audit ledger，记录根因、owner、处置与验证列；保存两侧 crate
  dependency graph 和 Runtime identifier 引用作为 before 基线。

Exit：主工作目录 HEAD 以 efdfa5dc 为祖先，source worktree 固定 7070f6b0；最终审阅后 Trellis task
active，冲突与依赖基线可复现。

## Phase 1 · Canonical Operation And OperationScript Foundations

- 物理搬运 Operation domain、execution core、authority/placement/result/audit、provider ports 和 core tests。
- 将 crate 收敛为 agentdash-application-operation-gateway。
- 搬运 OperationScript ports、bounded Rhai engine、preflight token 与 core tests。
- 保留 standalone UserWorkshop/Canvas gateway，但将 MCP/Extension/Interaction/Workflow provider 接入留到
  下一阶段，避免 core snapshot 与 provider composition 混在同一提交。
- 删除 PR 旧 Session/Extension action gateway。

提交：

- feat(operation): 搬运 canonical Operation execution core
- feat(operation-script): 搬运 async Rhai 组合执行器

验证：Operation core/provider/script focused tests，crate dependency scan 不含 Managed Runtime。

## Phase 2 · Physical Transplant Business Capabilities

按依赖顺序以 Git path snapshot/低冲突主题 commit 搬运：

1. Channel V2 domain/service/provider/persistence，不搬旧 AgentRun mailbox delivery。
2. Interaction domain/application/repository/contracts，不搬旧 AgentFrame/runtime surface 接线。
3. Extension Component ABI/toolchain/isolated host/exact artifact runtime。
4. WorkspaceModule projection-only tree，不搬旧 runtime bridge/tools。
5. MCP、Extension、Interaction exact Operation providers 与 Workflow OperationScript caller，各自独立接入。

每组先检查 source path digest/diff，搬运后只修 owning crate 的 manifest、exports/imports 和 module-local API。
每组同时检查业务 core manifest 与 public API 未新增 Managed Runtime/Host/Wire 依赖；发现引用时先在
ledger 判定其为必要 port 坐标还是具体实现泄漏，再决定搬运或重接。

提交：

- feat(channel): 搬运 V2 领域与 provider 基线
- feat(interaction): 搬运 shared Interaction 最终模型
- feat(extension): 搬运 Component 与标准 webview runtime
- refactor(workspace-module): 搬运 projection-only 模型
- feat(mcp): 接入 exact Operation provider
- feat(extension): 接入 exact Operation provider
- feat(interaction): 注册 command Operation provider
- feat(workflow): 接入可信 OperationScript caller

验证：各领域 focused tests、crate dependency scan、无旧 Canvas/runtime bridge/mailbox 静态残留。

## Phase 3 · Runtime Bridge Interfaces

- 在 application ports 定义 AgentOperationSurfacePort、BoundOperationToolExecutor 和
  ChannelAgentDeliveryPort。
- 实现 Business Surface descriptor -> ToolContribution adapter。
- 实现 binding-scoped runtime tool -> exact OperationRef registry。
- 实现 Tool Broker executor -> trusted Operation invocation adapter。
- authority 从 AgentRun binding/current AgentFrame/attachment 解析。
- trace/idempotency 使用 Runtime Thread/Turn/Item，不出现 RuntimeSession alias。
- 按最终决策实现 OperationScript nested invocation。
- 将 Runtime-specific imports 收敛到 adapter/composition allowlist，并为三个 port 使用 fake 实现补齐
  独立 contract tests。

提交：

- feat(agent-runtime): 投影 exact Operation tool surface
- feat(agent-runtime): 桥接 Tool Broker 与 Operation Gateway
- feat(operation-script): 接入 Agent Runtime 可信调用上下文

验证：surface digest/applied ack、stale binding/generation/tool-set、exact ref、nested re-admission、
cancel/timeout/idempotency。

## Phase 4 · AgentRun, Interaction And Channel Composition

- Interaction attachments 以 run_id + agent_id 为 subject，Surface compiler 查询 current attachment。
- Agent-facing Interaction commands 通过 dynamic Operation provider 进入 Surface/Tool Broker。
- WorkspaceModule 只投影 module/presentation/ref，不注册第二套 tool authority。
- Channel V2 delivery adapter 改接 PR AgentRun mailbox/facade。
- Extension/Channel/Runtime driver contributions 在 Integration trait 中正交组合。
- API AppState/bootstrap/routes/repositories 只采用 PR composition root 后追加新 providers。

提交：

- feat(interaction): 接入 Agent Runtime surface attachment
- feat(channel): 接入 AgentRun runtime delivery facade
- refactor(api): 统一 Runtime 与 Operation composition root

验证：Interaction command admission、Channel ingress/delivery、AgentRun launch/follow-up、Runtime
disconnect 不影响业务事实。

## Phase 5 · Migrations, Contracts And Frontend

- 将 Channel/Interaction migrations 改为 0066/0067。
- fresh PostgreSQL 执行 0061–0067。
- 合并 Rust contract generation registry，重新生成全部 TS/schema。
- 以 PR AgentRun feed/workspace 为基线接入 Interaction/Extension UI。
- 物理搬运 Canvas/Interaction frontend、promotion package/tests，再按最终 generated contract 修复调用面。
- 搬运 Canvas promotion E2E，保持 standard webview/exact provenance。

提交：

- feat(migration): 落地 Channel 与 Interaction 最终 schema
- feat(contract): 生成 Runtime 与 Interaction 统一合同
- feat(frontend): 接入 Interaction 与 Extension runtime

验证：migration guard、contracts check、frontend check/tests、focused E2E。

## Phase 6 · Specs, Cleanup And Full Gates

- 更新 Agent Runtime facade/surface/tool broker、Operation Gateway、Interaction、Channel 与
  frontend/backend contracts。
- 完成 coupling audit ledger：95 个重叠路径逐项关闭，汇总结构性耦合、必要装配、并行 cutover 与
  协作/生成冲突的 before/after 数量和最终证据。
- 静态扫描 RuntimeSession、旧 Canvas、旧 Extension action、old WorkspaceModule bridge、
  protocol_channels 和 authority injection。
- 通过 `cargo metadata` 与 source scan 验证 Operation、Interaction、Channel、WorkspaceModule core
  不依赖 Runtime/Host/Wire，Runtime 具体类型仅存在于 adapter/composition allowlist。
- 审查 AppState、integration registry、contract generator 与 crate exports，确认其仅包含注册/导出，
  不承载 provider admission、authority、attachment 或 delivery 业务规则。
- 运行：

      cargo fmt --all -- --check
      cargo check --workspace --all-targets
      cargo clippy --workspace --all-targets -- -D warnings
      cargo test --workspace --no-fail-fast
      pnpm run migration:guard
      pnpm run test-support:guard
      pnpm run contracts:check
      pnpm run frontend:check
      pnpm run frontend:test
      pnpm run check:quick
      git diff --check

- 使用 trellis-check 独立复核 physical transplant 完整性、bridge 依赖方向和 PR invariants。

提交：

- docs(architecture): 收敛 Agent Runtime 与 Operation 桥接合同
- test(integration): 完成 Runtime 与 Workspace 全量验收
- docs(task): 完成冲突适配任务验收

## Commit Construction Strategy

集成分支从 `efdfa5dc` 保持线性历史，不创建 merge commit。规划提交直接落在主工作目录的 integration
branch；产品提交按以下四层构造：

| 层次 | 内容 | 允许修改的共享面 |
| --- | --- | --- |
| Business payload | 最终领域/应用/仓储/测试快照及 owning crate 的 manifest/exports 修复 | 仅 owning crate；不接 Runtime |
| Provider integration | MCP/Extension/Interaction/Workflow 对 canonical Operation 的接入 | provider-local composition 与 tests |
| Runtime bridge | Surface、Tool Broker、Channel delivery adapters 与 contract tests | 明确 adapter + AgentRun facade |
| Root/finalization | AppState composition、migration、contract generation、frontend、specs | 每个主题分别提交，不形成收尾杂包 |

具体规则：

1. 每次只从 `7070f6b0` restore manifest 中的一组路径；先对照 source snapshot，再做 PR 基线所需的
   module-local 修编译。
2. 只有 owning crate focused check 通过后才 stage；显式按路径 stage，并检查 cached name-status、stat、
   diff 与 `diff --check`，不跨主题使用全工作区暂存。
3. 共享 root 文件延迟到 owning composition commit：`app_state.rs`、bootstrap/integrations、contract
   aggregator、migration registry 和 generated outputs 不夹进 payload commit。
4. 提交标题遵循 `type(scope): 可保留英文专业用词的中文提交信息`；正文至少分点记录搬运来源/语义、
   边界处置和已通过的 focused checks。
5. 不产生 `fix: 修复编译`、`merge: 解决冲突` 一类尾包。阶段内发现的问题回收到 owning commit；阶段
   checkpoint 前整理完历史再推送，checkpoint 后的真实语义修正继续使用对应领域 scope。
6. 每个 Phase gate 通过后推送一次远端 checkpoint，使远端始终保存已验证的线性主题历史。

示例：

    feat(interaction): 搬运 shared Interaction 最终模型

    - 从 7070f6b0 搬运领域、应用、仓储与合同源文件
    - 保持 run_id + agent_id attachment，不引入 RuntimeThread 依赖
    - 通过 Interaction focused tests 与依赖方向检查

## Review Gates

- 每个阶段只允许一个主题的业务变化。
- physical transplant 后先证明与 7070f6b0 对应目录语义一致，再做 adapter 修改。
- 每个原重叠路径必须在 coupling audit ledger 中有根因、canonical owner、最终处置和验证证据。
- 任何需要恢复 RuntimeSession/旧 Canvas/旧 gateway 的情况都返回 design review。
- Runtime bridge 变更不得扩散进 Interaction/Channel/WorkspaceModule domain。
- Runtime-specific 变更若越出 adapter/composition allowlist，必须先回到 design review 判断 port 是否缺失，
  不能以修编译为由直接扩大依赖面。
- migration 与 generated contracts 在产品代码稳定后统一落地。

## Completion Evidence

- 95 个重叠路径已在 `research/coupling-final-audit.md` 逐项关闭，final tree 与 canonical owner 可复核。
- Operation Gateway、Interaction、Channel、WorkspaceModule core 的 manifest/source scan 未发现 Managed
  Runtime、Driver Host 或 RuntimeWire 依赖；具体 Runtime 坐标位于 AgentRun adapter 与 API composition。
- final PostgreSQL readiness 不再要求 0067 已删除的 Canvas 表；0061–0067 migration guard 与 Runtime
  repository 集成测试通过。
- `cargo fmt/check/clippy/test`、migration/test-support/contracts/frontend gates、`check:quick` 与
  `git diff --check` 全部通过。
