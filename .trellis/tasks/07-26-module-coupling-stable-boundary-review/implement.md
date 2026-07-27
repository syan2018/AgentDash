# 模块耦合与稳定边界全量评估执行计划

## 前置状态

- 本任务当前处于 planning。
- 并行 research 可以在规划阶段完成，用于收敛评估口径和执行范围。
- 用户审阅最终规划摘要并在后续消息明确批准前，不运行 `task.py start`，也不编写最终综合报告。

## 执行步骤

1. **固定评估基线**
   - 记录开始审计时的 commit、分支和任务目录以外的并行修改。
   - 将 Agent Runtime 事故样本与全局评估问题分开记录。

2. **完成第一轮四路证据采集**
   - 后端领域、应用、基础设施、入口和持久化。
   - 前端、跨层合同、Cloud / Local / Desktop 投影。
   - workspace/package graph、git churn、co-change 和测试门禁。
   - 历史 review、Trellis 任务与会话中的已知事故链。

3. **完成后端全业务三路深审计**
   - 业务资产与所有权：逐 use case 审计 CRUD、发布、安装、删除、授权和跨聚合事务。
   - 控制编排与协作：逐 use case 审计 reducer、gate/wait、派发、审批、恢复与 policy owner。
   - 执行与系统装配：逐 use case 审计 Product/Runtime/Driver、Hook/VFS/Tool、API/Local/Relay、
     persistence/composition。
   - 三路均需从 production entrypoint 追到 owner/persistence/consumer/test，不以目录抽样结束。
   - 独立完成 architecture enforcement 可实施性复核，逐项确认权威输入、检查形态、现有 gate
     接入点和 guard self-test；不把“写一个脚本”直接计作已形成约束。

4. **建立 business coupling matrix 与 boundary inventory**
   - 以 `research/backend-entrypoint-coverage-index.md` 为清单，枚举 router、MCP/tool catalog、worker、
     startup、relay/local/Tauri command 的 production entrypoint。
   - 为主要 command/query 填写授权、command owner、read/write owner、transaction/effect、contract、
     recovery、consumer 与 gate。
   - 按业务能力而非目录列出 owner、command、read model、persistence、protocol、UI 和 tests。
   - 对仓库覆盖表逐格标记：已核验、无生产路径、证据不足。
   - 标出名义边界与真实稳定边界不一致的位置。

5. **交叉验证候选问题**
   - 每个 P0/P1 至少由两类证据支持，其中一类必须是当前 production code 或会失败的测试。
   - 核对 consumer 是否真实进入 production composition，避免把未挂载代码当影响面。
   - 用 git history 判断问题是 residual、resurfaced、new 还是 already converged。
   - 明确高连接 hub 是否是合理 composition root，避免把必要集成误判为耦合。

6. **编写综合报告**
   - 给出全局边界地图和代表性纵向链路。
   - 按 P0/P1/P2 列出发现、证据、变更触发器、爆炸半径、根因和目标边界。
   - 单独回放 Agent Runtime 样本，说明哪些全局规则能解释事故、哪些不能。
   - 列出合理依赖和应保留边界，防止整改演变为机械拆 crate。
   - 初始化 `architecture-stability-ledger.md`：current/target/proven 三图、Stability Delta、
     module coverage和evidence index；初始 proven 图只包含已有充分行为证明的边界。

7. **设计防复发 architecture enforcement**
   - 写出 boundary manifest / owner ledger 的数据模型和 owner。
   - 为 dependency、public API、semantic transaction、generated contract DAG、route/composition
     completeness、schema owner、frontend dispatcher、desktop IPC/path 分别定义 guard。
   - 指定每个 guard 的 PR/contract/desktop/deployment gate 入口、失败消息和 self-test。
   - 明确 exception 不是自由 allowlist：例外必须有 owner、理由、到期/删除条件和测试。

8. **规划整改波次与 work package graph**
   - Wave 0：Architecture harness/self-test、workspace-derived root、entrypoint/data/contract inventory；
     同时固定 P0 的成功/失败/并发/重启 characterization。
   - Wave 1：先封直接安全与 production 断链：public diagnostics、workspace setup admission、Relay
     header auth、Interaction effect worker、Companion delivery、Gate terminal convergence。
   - Wave 2：收敛 canonical write/operation authority：LifecycleRun command store → Task/dispatch，
     Project grant transaction、Project semantic retirement、asset operations、Routine occurrence、
     dynamic tool generation lease。
   - Wave 3：统一多入口与恢复合同：Story/Workflow command、OAuth/Runner receipt、Wait/Channel、
     Terminal launch、Local bounded dispatch、Hook/MCP/execution profile contract。
   - Wave 4：并行收口 frontend/generated boundary：live delta、Workspace presentation、Project event
     dispatcher、JSON integer/runtime decoder、Tauri/path/Terminal port。
   - Wave 5：在 semantic port 稳定后执行 Platform SPI、persistence/composition、VFS adapter、
     application aggregate/AppState/RepositorySet hard cut。
   - Wave 6：各违规归零后启用 G0-G9 blocking gates，清理 dead protocol/facade/re-export/失效
     spec/guard root，并用 release attestation闭合 PR/cloud/desktop/deployment。
   - 以 `research/backend-cross-audit-synthesis.md` 的合并节点和
     `research/architecture-enforcement-feasibility.md` 的 WP-A0～A8 为最小依赖基线，不按三份报告
     分别创建重复子任务。
   - 每个建议拆成可独立验收的 Trellis 后续任务候选，并写出依赖顺序、文件所有权、允许/禁止范围、
     迁移/删除项、验证命令、负向门禁、完成定义和回滚点。
   - 输出 parent/child task map；计划通过后再创建实际子任务，避免未经审阅就扩张 active task 树。
   - 每个 child task 创建时同时初始化 `boundary-proof.md`，先写 Behavior Boundary Contract 和
     proof obligations，再允许修改产品代码。
   - 父任务在每个 child 完成后执行跨任务 integration check，确认行为证明、old path absence、
     production composition与blocking gate，再更新 architecture stability ledger。

9. **质量核查**
   - 检查 PRD 的每条验收标准是否在报告中有对应章节或矩阵。
   - 检查所有 P0/P1 的文件锚点仍存在且 production 可达。
   - 检查后端主要业务域的 production use-case ledger 没有只用“抽样”或 crate fan-out 代替。
   - 检查报告没有把 spec、依赖数量或 git 共变单独当成结论。
   - 检查每项整改都能映射到 work package 和至少一个自动门禁，且依赖图无循环。
   - 检查每个 guard 对不存在 owner/root、非法依赖或缺失 binding 都有失败 fixture。
   - 检查每个 work package 都有 finding/change-trigger/invariant/owner/blast-radius/proof 六类锚点。
   - 检查 preserve/correct/remove 行为已经明确，不把当前错误 characterization 变成兼容要求。
   - 检查 module → work package → invariant → proof owner 无遗漏、无双 integration owner。
   - 检查 ledger 的 proven 状态都有 child proof、production composition、old path absence 与
     required gate；未证明项仍显式可见。
   - 对代表性 change simulation 复核预期修改面，不能只用 diff 行数或 crate 数证明稳定性。
   - 检查没有修改任务目录以外的任何文件。

## Work package 记录格式

`convergence-plan.md` 中每个节点使用同一结构，不能只写标题和一句建议：

| 字段 | 必填内容 |
| --- | --- |
| ID / title | 稳定、可作为 Trellis 子任务名的标识 |
| anchors | finding、production symbol、change trigger、target invariant、owner、blast-radius claim |
| target invariant | 完成后唯一 owner、允许依赖方向、事务/恢复和 public contract |
| prerequisites | 必须先完成的 work package；没有则显式写 none |
| owned files / modules | 本节点负责的边界；与并行节点不重叠的文件所有权 |
| behavior disposition | 哪些行为 preserve、correct、remove |
| behavior matrix | actor/scope、success/error、concurrency、idempotency、restart/replay、consumer |
| characterize | current行为证据；已知错误对应的旧实现失败 fixture |
| migration / cutover | authority 如何一次性移动、consumer 顺序、schema migration |
| hard delete | 旧 producer、repository access、DTO/mapper、route、fallback、re-export |
| boundary proof | owner contract、failure injection、concurrency/recovery、composition、E2E、absence |
| validation | 定向 unit/contract/integration/E2E/desktop 命令与预期结果 |
| negative gate | 复发时必然失败的 fixture/rule及其 CI gate |
| completion | production composition 可达、旧路径为零、spec/manifest同步 |
| blast-radius result | 哪些 consumer 以后不再理解被隐藏实现 |
| ledger delta | current/target/proven 图、Stability Delta 和module coverage如何更新 |
| rollback point | 仅描述实施中可回到哪个未切 authority 的提交点；不保留运行期双轨 |

Graph 约束：

- P0 admission/数据正确性节点只依赖 characterization 和必要的数据 owner 基础，不等待物理目录整理；
- 同一 canonical fact 的 authority cutover 只能有一个节点负责，避免两个子任务各建 facade；
- crate/package split 必须依赖相关 semantic port 已确定，不能把现有 RepositorySet/ServiceSet 原样搬家；
- gate 节点不能先于违规归零标记完成；修复与 blocking gate 在同一节点或显式相邻节点闭合；
- 最终 hard cleanup 节点只处理已无 production consumer 的结构，不承担新的业务语义决策。
- work package 大小不设上限；若一次 authority cutover必须跨多个模块才能保持行为原子，就保持同一
  工作项并用内部检查点验证，不为缩小 diff 拆出运行期双轨。
- child task 只有在 `boundary-proof.md` 通过父任务集成复核后才能完成；局部测试通过但ledger仍是
  `cutover` 的任务不能归档。

## Boundary Proof 执行节奏

每个 child task 使用相同阶段：

1. **Baseline**：在 `boundary-proof.md` 固定 anchors、Behavior Boundary Contract和current evidence。
2. **Proof first**：补会在旧路径、partial failure、stale writer、缺binding或错误consumer解释时失败的
   fixture；不盲目 snapshot 当前错误行为。
3. **Authority cutover**：移动唯一owner/transaction/recovery identity并迁移全部production入口。
4. **Hard delete**：删除旧producer、fallback、mapper、registration和compatibility re-export。
5. **Local closure**：运行本工作项的contract/failure/concurrency/composition/E2E/absence checks。
6. **Parent integration**：父任务运行跨work-package纵向链路、change simulation和共享gate。
7. **Ledger promotion**：先更新Target；只有proof、absence和blocking gate齐全后更新为Proven。

父任务每个wave结束时生成一次可审阅快照，列出：

- 新增 proven boundary；
- 被删除的跨模块知识和旧依赖边；
- 当前仍未证明的target boundary；
- change simulation的实际修改面；
- 下一wave前置条件是否真实满足。

## 验证命令

```powershell
git status --short
rg -n "P0|P1|P2|证据不足|合理边界" .trellis/tasks/07-26-module-coupling-stable-boundary-review
rg -n "TODO|TBD" .trellis/tasks/07-26-module-coupling-stable-boundary-review
py -3 ./.trellis/scripts/task.py current
```

根据报告中的具体边界追加定向依赖搜索、contract check 或测试枚举；本评估任务不运行无关的全量构建和测试。

## 风险与停止条件

- 若 production composition 无法证明某模块被使用，将其列为“可达性证据缺口”，不判断真实爆炸半径。
- 若并行会话正在修改同一证据文件，只读取 commit 基线或记录观察差异，不覆盖其改动。
- 若最终发现需要产品语义选择，返回 planning 更新 PRD；不在评估报告中替用户决定。
