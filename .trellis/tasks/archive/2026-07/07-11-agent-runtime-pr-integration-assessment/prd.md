# Agent Runtime 重构冲突适配评估

## Goal

以 PR #93 Agent Runtime 为执行基线，在主工作目录中建立集成分支，并从旁路 source worktree 物理搬运
Workspace/Channel 分支的最终业务能力；用窄接口重新连接 Runtime 与 Operation、Interaction、Channel、
WorkspaceModule，同时以现有冲突清单为样本完成耦合根因审计，得到不存在双事实源、Runtime 变更可被
局部吸收且可通过全量门禁的统一架构。

## Background

- Workspace/Canvas V1 与 Channel 重构已按主题提交并归档，当前分支 clean。
- 当前分支已推送到 `origin/codex/workspace-duplex-interaction-planning`，远端存档点为
  `7070f6b0`。
- 本分支将 Runtime Gateway 收敛为 canonical Operation，删除 Session-bound action gateway；
  RuntimeSession 只保留 AgentRun connector delivery、trace evidence 与必要执行关联。
- 用户指出远端某个 PR 对 Agent Runtime 做了另一轮重构，需要先定位 PR，再评估双方的事实源、
  public contract、migration、调用链和提交整合顺序。
- 唯一候选已确认为 PR #93
  `codex/agent-runtime-architecture-convergence@efdfa5dc`，base 为
  `main@957fa9d6`；GitHub 显示 mergeable，quick-check 与 deployment-contract 通过。
- 当前分支与 PR #93 共用 merge-base `957fa9d6`，分别有 73 与 11 个独有 commits；双方修改
  356/626 个文件，其中 95 个路径重叠。只读 `git merge-tree` 已确认大量 content 与
  modify/delete 冲突，不能按普通 merge conflict 逐文件取一侧。
- migration 编号直接冲突：当前分支使用 0061/0062 落地 Channel V2 与 Interaction，PR 使用
  0061–0065 落地 Managed Runtime、Tool/Hook/Driver Host 与 cutover。

## Requirements

1. 使用本地 `gh` 定位候选 PR，并以 PR head/base/merge-base 和原始提交为证据，不凭标题猜测。
2. 对比 PR 与当前分支在以下边界的差异：
   - AgentRun、AgentFrame、RuntimeSession 与 connector execution identity；
   - runtime launch、follow-up/resume、terminal/tool/MCP 与 trace projection；
   - canonical Operation/OperationScript/Extension/Interaction callers；
   - API/contract/generated TS、database migration、tests 与 specs。
3. 区分文本冲突、可机械移植改动、语义冲突和互相覆盖的删除/重构。
4. 明确推荐的整合基线、提交顺序、需要重做而非保留的适配点、验证门禁和可回滚点。
5. 保持预研项目的最终正确模型，不设计兼容层、双路径或 legacy fallback。
6. 评估阶段只允许读取/获取远端 refs 和创建规划文档；实现需在最终规划审阅与
   `task.py start` 后开始。
7. 主工作目录 `F:\Projects\AgentDash` 切换为从 PR #93 head `efdfa5dc` 创建的集成分支；当前
   Workspace/Channel 存档分支固定在 `F:\Projects\AgentDash-workspace-duplex-source` source worktree，
   作为只读搬运来源，不在其上 merge/rebase 或继续修改产品代码。
8. 默认采用“物理搬运优先”：
   - 无 Runtime 耦合的新增目录、领域模型、应用用例、前端、Extension toolchain、tests 和 specs
     直接从当前分支搬运最终文件或重放主题提交；
   - 仅对 PR 已删除/替换的 runtime/session/mailbox/surface/tool 路径重新接线；
   - 不以逐文件手写方式重建已经验证过的业务实现。
9. Runtime 桥接必须通过窄接口隔离，使 Operation/OperationScript、Interaction attachment、
   WorkspaceModule 和 Channel 不依赖 Managed Runtime、Driver Host 或 RuntimeWire 的内部结构；
   后续 Agent Runtime 重构应主要替换 adapter/composition，而不触及业务领域实现。
10. Agent OperationScript 在 Managed Runtime 中只产生顶层 ToolCall Item；nested Operations 每次重新
    admission，并仅在 Operation core 中记录 child trace/audit，不创建 child Runtime Item。
11. 将 95 个重叠路径（含 55 个显式冲突与 40 个自动合并路径）作为模块独立性审计样本，逐类区分：
    - Runtime 具体类型或旧执行流程泄漏进业务模块形成的结构性耦合；
    - AppState、contract registry、crate exports、migration 等合理但应保持薄层的集中装配点；
    - 双方同时删除或替换旧架构形成的并行 cutover 冲突；
    - specs、journal 与 generated artifacts 等协作/生成冲突。
12. 本次迁移必须给出冲突面的 before/after 结论：Operation、Interaction、Channel 和 WorkspaceModule
    的领域/应用核心不依赖 Managed Runtime、Driver Host 或 RuntimeWire；Runtime 具体类型只允许存在于
    明确的 adapter 与 composition root；合理共享装配点只承载注册和接线，不承载业务规则。
13. 对无法在本次迁移中消除的共享修改点记录其事实源、变化原因和最小接口，并用 dependency scan、
    focused tests 或结构检查证明后续 Runtime 重构不需要再次修改业务核心。
14. 集成分支保持从 PR head 开始的线性主题历史：业务快照搬运、provider 接入、Runtime bridge、顶层
    composition、migration/contracts/generated artifacts 分层提交；每个提交使用项目规定的中文主题格式
    与分点备注，并在 focused gate 通过后才进入 phase checkpoint，禁止形成笼统的 conflict-resolution
    或 build-fix 汇总提交。

## Acceptance Criteria

- [x] 唯一目标 PR 已定位，记录 URL、base/head SHA、状态和关键提交。
- [x] 建立 PR 与当前分支的 merge-base、双向 commit/file diff 和冲突热点清单。
- [x] 每个热点说明双方意图、目标事实源、保留/重做/删除结论和验证证据。
- [x] 形成不依赖兼容方案的推荐整合架构与分阶段实施顺序。
- [x] 形成逐目录/主题的 physical transplant manifest，明确直接搬运、搬运后修编译和重新桥接三类。
- [x] 主集成 checkout、source worktree、目标分支名、PR head 锚点与回滚方式明确。
- [x] Runtime bridge ports 的输入输出、依赖方向和唯一 composition root 明确。
- [x] `design.md` 覆盖领域边界、数据流、migration/contract 处理和整合策略。
- [x] `implement.md` 覆盖主题化提交、验证命令、风险与回滚点。
- [x] 冲突清单已按结构性耦合、集中装配、并行 cutover 和协作/生成冲突建立初步分类。
- [x] 实施完成时，95 个重叠路径均有根因与目标处置，且形成可复核的 before/after 耦合审计。
- [x] Operation、Interaction、Channel、WorkspaceModule 的核心 crate 通过依赖方向检查，不引用 Managed
      Runtime、Driver Host 或 RuntimeWire；Runtime 类型仅出现在约定 adapter/composition allowlist。
- [x] 集中装配点保持为薄注册层，Runtime-specific 业务规则不进入 AppState、contract registry、crate
      exports 或 migration orchestration。
- [x] 实施顺序与 commit construction strategy 已明确，可将 source snapshot、bridge 和 generated 变更
      分开审阅。
- [x] 最终提交历史按领域/接缝分层，每个提交可说明来源锚点、边界变化与 focused 验证，不存在混合多
      主题的 merge/fixup 尾包。
- [x] 用户已审阅并确认最终方案，可以激活任务。

## Out Of Scope

- 在评估阶段直接合并远端 PR 或修改产品代码。
- 为旧 Session-bound runtime 或旧 Canvas/Extension action contract 增加兼容层。
- 顺带修复与两条分支整合无关的 Agent loop、前端 lint 或 E2E 基线问题。
