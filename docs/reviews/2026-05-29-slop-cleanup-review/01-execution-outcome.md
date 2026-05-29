# 2026-05-29 slop 清理 — 执行结果

> 分支 `refactor/architecture-slop-cleanup`（未 push，待人工 review）。本文件记录自主执行的落地结果，与 `00-synthesis.md`（审查发现）配套。

## 元结论（最重要的一条）

**三个最深的"病灶"在执行调查中被证明高估了耦合。** review 由只读 agent 完成，把"不同契约/不同阶段的代码"误判为重复、把 live 代码误判为死代码。执行 agent 在动手前调查并据实修正：

| 病灶 | review 主张 | 调查实情 | 处置 |
|---|---|---|---|
| 2 drop-step | Step lifecycle 是死代码可删 | 是 task 启动/续跑/companion/VFS 投影的 **load-bearing 活跃 runtime**，Activity 轨无对应入口 | **冻结**，未改代码，记录 P0a/b/c 迁移路径 |
| 3 session-assembly | compose_* 与 plan_* 重复六步链 | 两条路径是**不同输出契约**（launch 产 bundle/capability_state，query 产只读快照丢弃 cap_output），收敛点已存在 | 不强抽 SessionSurfaceResolver，仅删真正的死镜像 `projections.context`（5 写 0 读） |
| 4 capability | 四处散落冗余 + 两套 dimension trait 重复 | 四处是 resolve→transition→derive→render **四阶段分工**；两 trait 正交（effect 应用 vs 渲染） | 不合并 trait/不建单一 service，仅把 delta 纯数据类型上移 spi |

**而机械/结构类发现全部准确并干净落地**：infra 82% 重复（实测属实）、application 基础设施泄漏、类型去重/改名。

教训：静态 review 善于发现"形似重复"，但区分"刻意分离"与"冗余"需要执行期的数据流验证。**预研期激进删除的前提是先确认是不是死代码。**

## 已落地 commit（11 个，均过 build-gate + 测试）

| commit | 内容 | 验证 |
|---|---|---|
| f62b5d89 | review 文档 + 任务树 | docs |
| 29eea9ad | drop-step 冻结记录 + 波次修订 | docs |
| 7438461e | 前端 A/B：react-query + service 层 | tsc + 270 测试 |
| 045a34ca | 修预存 bug：routine-memory 漏登记版本 | 6 测试转绿 |
| 42cb3a35 | dedup：McpTransportConfig 去重 / SessionPersistence supertrait / VfsService 改名 / bridge & MCP helper（55 文件，净 −78） | check + 653 测试 |
| 3d6e4326 | 前端 workspace-layout god component 拆分（1230→442） | tsc + 270 测试 |
| e0e5c08a | 病灶 1：基础设施下沉 5 SPI port（rhai/rmcp/reqwest/tokio::process/MemoryPersistence），35 文件 | check + 656 测试 |
| 9432f988 | 病灶 5：session_repository sqlite/postgres 去重，抽 session_core（净 −531，1300 行重复收敛）+ status 下推 | check + 52 测试 |
| 699b11cc | 病灶 3：删 ConstructionProjections.context 死镜像 | check + 604 测试 |
| 4ff640fb | 病灶 4：capability delta 类型上移 spi | check + 604 测试 |

最终测试态：spi 68 / application 604 / infrastructure 52 / executor 47 / 前端 270 全绿；agentdash-api 88 绿。

## 已知遗留（需人工处理，非本次回归）

1. **预存失败测试** `agentdash-api vfs_access::tests::runtime_tool_schemas_are_openai_compatible`：断言 fs builtin 工具 schema `offset` 必填。本次重构对 vfs_access 仅做 `RelayVfsService→VfsService` 改名（已核 diff，零触及 schema/offset），与本任务无关。属 fs-tools-align 范畴的历史不一致，建议在该专项处理。
2. **drop-step 冻结**：删 Step lifecycle 需先做 P0a（Activity 版 task-start/companion 入口）→ P0b（provider_lifecycle/journey/advance_node/construction 投影改读 activity_state）→ P0c（删 domain Step + migration）。这是 feature 迁移，需人工设计与决策。
3. **前端 god component**：仅 workspace-layout 拆完；activity-inspector(1304)、SettingsPageContent(2014) 的拆分与 stage C（@agentdash/ui 基线）因 agent 反复 stall 未完成，留待后续小步处理。
4. **3 个高风险 commit 建议人工 review**：699b11cc / 4ff640fb（及它们的调查结论），尽管测试等价。

## 建议后续

- `surface.vfs` / `context_projection.vfs` 的"单存储+派生"合并：两次调查都判定高 blast-radius、零行为收益，暂不动。
- `compose_owner_bootstrap`/`compose_story_step` 可做**路径内**按阶段拆小函数（低风险，不跨路径合并）。
- contracts 层的第三份 `McpTransportConfig` 是否合并：属 API 契约 DTO 边界问题，单独评估。
