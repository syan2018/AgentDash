# 模块耦合与稳定边界全量评估

## Goal

对 AgentDash 当前仓库进行一次以“稳定边界是否真实可维护”为核心的全量模块耦合审计，并把审计结果
转化为可直接执行的全项目耦合收敛计划。计划必须同时解决当前依赖方向、事实权威、协议所有权、
持久化职责和跨层投影导致的变更爆炸半径，并建立可自动执行的架构与边界约束，防止同类问题再次出现。
此前 Agent Runtime / AgentRun 重构产生的大范围连锁破坏只作为一个已知事故样本，用于验证评估方法
和防复发机制能否解释真实故障，不构成本次审计的中心范围或优先级来源。

本任务的价值不是统计依赖数量，而是回答：

- 哪些边界目前只是目录或类型层面的名义边界，无法隔离业务变化；
- 哪些概念由多个模块共同解释、共同推进或重复持有事实；
- 哪些稳定契约缺失，使任一核心能力变化穿透不应理解其内部语义的相邻模块；
- 应先收敛哪些边界，才能让后续模块演进具备可预测的影响范围。
- 每个收敛动作如何拆成有依赖关系、可独立验收且不会再次打碎业务主链的工作包；
- 哪些架构规则必须从文档升级为 CI、contract、composition、transaction 和 recovery 门禁。
- 如何证明每个模块/工作项真的恢复了正确行为边界，而不是只完成文件移动、编译通过或局部测试。
- 如何持续、直观地展示重构前后 canonical owner、依赖方向、事务/恢复和 consumer 知识面的改善。

## Background

- 项目此前进行 Agent Runtime 重构时，与 AgentRun 关联的多个模块发生连续断裂，需要逐层修复；这是
  用户用来说明“稳定边界失效”的例子，而不是把 review 限定到 Agent Runtime 的要求。
- 当前 `.trellis/spec/` 已记录大量 Runtime、Product facade、persistence、driver、wire、frontend
  projection 契约，说明该领域存在显著的跨层协同成本。
- 本次评估以全仓所有业务域和基础设施边界为同等候选对象，先按统一标准审计，再根据证据确定风险排序。
- Agent Runtime / AgentRun 仅在全局扫描完成后作为事故回放样本，检查高风险规则是否具备解释力。
- 第一轮并行研究只完成了仓库结构、后端横向热点和前端/跨层审计；它不足以替代整套后端业务逻辑的
  逐域核验。本轮继续按业务资产、控制编排、执行装配三个面审计 production use case。

## Requirements

- 建立仓库模块与分层清单，覆盖 Rust workspace、前端 packages/features、数据库与 migration、
  本机/云端/桌面运行边界、生成契约和项目级规范。
- 采用并行全局审计：后端领域与基础设施、前端与状态投影、跨端/部署/协议边界、全仓依赖与协同变更
  热点分别独立取证，再进行交叉校验和统一排序。
- 同时评估静态依赖和语义耦合：import/crate 依赖、共享类型、事件/DTO、事实源、状态机、identity、
  generation、持久化、projection、composition root 与测试装配。
- 对每个主要业务域抽取至少一条关键纵向数据流，标注每一跳的 owner、输入输出合同、转换和失败语义；
  AgentRun 链路只是其中一条。
- 对整套后端业务逻辑建立 use-case coverage ledger，至少记录每个 production command/query 的入口、
  权限、具名依赖、读取 owner、写入 owner、事务边界、外部副作用、事件/投影、恢复语义和测试门禁。
- 后端业务审计按独立研究面并行完成，不能由单个横向审计结果代表；每个研究面必须基于 production
  route/application/domain/persistence/composition 真实调用链取证。
- 使用代码、测试、migration、现有 spec、相关 Trellis 任务、git 历史和本地会话历史作为证据，
  不以目录命名或架构文档声明替代实现核验。
- 识别循环依赖、反向依赖、跨层旁路、重复事实源、重复映射、过宽 facade、泄漏的内部类型、
  隐式 composition 约束、跨模块协同修改热点和缺乏架构测试的稳定边界。
- 区分“必要的业务内聚”与“应该被拆除的变化耦合”，避免仅以依赖数量判定设计质量。
- 为每项发现记录严重度、证据位置、受影响边界、典型变更触发器、爆炸半径、根因类别和建议的
  目标边界。
- 形成按依赖顺序排列的整改路线：先恢复事实权威和依赖方向，再收窄协议与 facade，最后处理局部
  结构和重复实现。
- 把整改路线写成可执行 work package graph。每个工作包必须包含目标 owner/boundary、前置依赖、
  允许和禁止修改范围、迁移/删除内容、验证命令、负向门禁、完成定义、预计缩小的爆炸半径和回滚点。
- 每个 work package 必须锚定至少一个当前 production finding、一个变化触发器和一个目标 invariant；
  没有证据锚点或只以“整理/拆分/清理”为理由的工作项不能进入执行图。
- 每个 work package 必须定义 `Behavior Boundary Contract`，逐项声明应保留、应纠正和应删除的行为，
  并覆盖入口/actor、输入输出错误、canonical fact、并发/幂等、事务/副作用、重启/重连/重放、
  consumer projection 与 production composition。
- 每个 work package 必须产出可执行 `Boundary Proof`：characterization、contract、failure injection、
  concurrency、restart/replay、composition reachability、代表性纵向 E2E 和 old-path-absence 证据；
  不能用编译成功、文件已移动或静态规则单独证明行为正确。
- 建立 module → work package → behavior invariant 覆盖矩阵；一个模块参与多个稳定边界时必须分别
  映射，不能以“该模块已重构”掩盖其中尚未验证的 use case。
- 维护父任务拥有的 `architecture-stability-ledger.md`，直观展示 current → target → proven 三个状态：
  当前泄漏/爆炸半径、目标 owner/contract、已删除旧知识、行为证明、blocking gate 与剩余证据缺口。
- 每个子任务完成后先提交自己的 `boundary-proof.md`，再由父任务集成检查更新稳定性账本；并行子任务
  不直接争写父账本。
- 重构范围和工作量不作为裁剪正确终态的理由。任务可以跨越任意数量模块和 migration；拆分只用于
  明确 authority cutover、文件所有权和可验证检查点，不允许以阶段交付保留双轨或未证明的半边界。
- 设计全项目 architecture enforcement system，至少覆盖 crate/package 依赖方向、公开 API ownership、
  RepositorySet/AppState service locator、跨聚合事务、generated contract DAG、composition completeness、
  production route reachability、migration/schema owner、前端 owner-scoped dispatcher 和跨端 IPC/path。
- 将可以独立实施的收敛动作组织为后续 Trellis 子任务树；父任务保存全局目标、依赖图和跨子任务验收。
- 本任务只负责完整审计、目标架构、执行计划、防复发机制和后续任务拆分，不直接实施产品代码重构。

## Acceptance Criteria

- [ ] 存在可审阅的仓库模块/边界地图，且覆盖 backend、frontend、cross-layer、runtime host/driver、
  persistence、database/migration 与 desktop/local/cloud 组合关系。
- [ ] 存在后端全业务 use-case coverage ledger，覆盖主要 production command/query，且每项能定位入口、
  owner、跨聚合读写、事务/副作用、投影和测试门禁；不能只用 crate graph 或抽样热点代替。
- [ ] 主要业务域各有代表性纵向链路，能够明确 canonical owner、projection owner、transport owner、
  persistence owner 与 UI owner；Agent Runtime / AgentRun 只是其中一条事故回放链。
- [ ] 每个高风险结论至少包含一个当前 production code、production composition、测试或 migration
  证据锚点；git、历史任务和架构声明只能作为补充证据。
- [ ] 报告明确列出当前最危险的耦合点、它们为何导致连锁破坏、哪些边界是假边界，以及哪些依赖是合理的。
- [ ] 报告包含按严重度和修复先后排序的行动清单，并说明每个阶段预期缩小的变更爆炸半径。
- [ ] 报告给出可自动执行的后续守护建议，例如依赖方向检查、contract drift、composition 测试、
  owner invariant 测试或架构规则，而不只依赖文档约束。
- [ ] 存在可执行的全项目收敛 work package graph；每个工作包有前置依赖、边界合同、修改范围、验证、
  负向门禁、完成定义和预计收益，可直接创建/启动对应 Trellis 子任务。
- [ ] 存在 architecture enforcement 设计，明确每条长期规则的权威输入、检查实现、CI 入口、失败信息、
  允许例外的 owner 和自检测试，确保架构文档、workspace、guard 与 production composition 同步。
- [ ] 存在“如何安全重构稳定边界”的统一执行协议，要求 characterization → authority cutover →
  consumer migration → old path deletion → negative gate，避免完成结构迁移却遗漏真实业务路径。
- [ ] 每个 work package 都有可审阅的 Behavior Boundary Contract，明确 preserve/correct/remove 行为，
  并能映射到具体 production entrypoint、actor、owner、failure/recovery 和 consumer。
- [ ] 每个 work package 都有 Boundary Proof 计划；完成判定至少包含行为测试、production
  composition、旧路径归零和 blocking negative gate，不能只依赖 build/typecheck/unit happy path。
- [ ] 存在 module → work package → behavior invariant 覆盖矩阵，所有受影响 production 模块/入口均
  能定位其验证责任；跨任务共享边界有唯一集成 owner。
- [ ] 存在并持续维护 `architecture-stability-ledger.md`，包含 current/target/proven 架构图、
  Stability Delta 表、每个边界的证据链接与未证明项，能够直观看出重构后具体更稳妥在哪里。
- [ ] 每个 child task 的 `boundary-proof.md` 经父任务集成复核后才能更新 ledger 状态为 proven；
  所有 high-risk boundary 在最终关闭前均达到 behavior proven + gate blocking + old path absent。
- [ ] 代表性 change simulation 能证明目标边界会吸收预期变化，例如新增一个 Project-owned fact、
  Runtime Tool executor、MCP/HTTP 入口、event variant 或 Tauri command 时，只修改 owner/contract/
  adapter及其门禁声明，不再穿透无关 consumer。
- [ ] 报告对尚无法证明的判断显式标注证据缺口，不把推测写成事实。
- [ ] 评估产物能够直接拆分为后续 Trellis 子任务，每项具备清晰边界和可验收结果。

## Out of Scope

- 在本任务中直接修改业务代码、数据库 schema、API 或前端交互。
- 在本任务中直接执行收敛 work package；它们在计划验收后作为后续子任务实施。
- 为保持旧结构可用而设计兼容层、回退路径或双写方案。
- 与模块边界无关的代码风格、视觉设计和一般性能优化。
- 仅凭行数、文件数量或依赖计数进行排名，而不解释业务语义与变化原因。

## Constraints

- 项目尚未上线，目标建议应以最正确的最终结构为准，不引入兼容性负担。
- 目标正确性与可验证性优先于重构规模、耗时和改动文件数；不能为缩小单个 PR 而保留运行期兼容层、
  双写、fallback 或第二 authority。
- 不触碰工作区中其他会话的修改；若证据受并行修改影响，记录观察时间与 commit。
- 文档只记录可复用的架构理由和证据，不记录无长期价值的任务流水。
