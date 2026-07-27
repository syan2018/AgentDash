# 历史事故样本与既有 Review 证据

## 目的

本文件只回答两个问题：

1. 过去哪些变化暴露过“名义分层存在，但稳定边界无法吸收变化”的现象；
2. 既有 review 已经覆盖什么、为什么不能直接当成本次全局评估结论。

Agent Runtime 是已知高强度样本，不是本次审计范围中心。历史任务数量、commit 数量和 co-change 只能用于
发现候选，不能单独证明架构错误。

## 既有 Review 不能替代本任务

### 06-14 模块过度设计 review

`.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` 已覆盖 Lifecycle、AgentRun、
VFS/Local/Relay/Extension、Frontend/Contracts/Permission，并识别重复事实源、过宽 composer 和前端重复
投影。其评估目标是“过度设计与第一轮清理”，且后续验收项已实施多轮收束。

本任务与它的差异：

- 评估单位从“过厚模块/重复抽象”改为“变化能否被稳定边界吸收”；
- 引入当前 workspace/package graph、git 共变和事故回放，而不是只看当时静态代码；
- 需要验证旧结论是 residual、resurfaced、already converged 还是被新的边界问题取代；
- 风险排序不预设 Lifecycle / AgentRun 为中心；
- 最终必须给出可自动守护的依赖、owner、composition 和 recovery gate。

### 05-14 Session Launch 重构

`.trellis/tasks/archive/2026-05/05-14-session-launch-refactor-assessment/prd.md` 将 owner、workspace、VFS、
MCP、capability、context、identity、restore、hook、pending command、terminal effects 收敛进
`LaunchCommand -> SessionConstructionPlan -> LaunchExecution`。父任务拆出多个 batch，并在最终文档中
持续追踪“唯一事实源”“旧 shell 删除”“persistence 语义边界”。

这提供一个重要候选症状：当一次 launch 需要跨多个业务 owner 获取信息时，如果稳定输入不是闭包后的
typed plan，而是多个中间 request、repository lookup 和 fallback，重构会被迫同时修改所有来源与消费方。
但是否仍是当前问题必须由当前 production code 重新验证。

### 07-10 Agent Runtime 架构收敛

`.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/prd.md` 的 confirmed facts
记录了当时 application runtime/session、application-agentrun、agent core、protocol、relay、hook 和
integration 之间的 ownership 与依赖问题。其
`research/application-compaction-context.md` 明确指出当时 crate 拆分主要是物理拆分而非信息隐藏，
并记录 launch/restore、context construction 和 persistence success boundary 的重复。

该任务随后建立 Product facade、Managed Runtime、Driver Host、Agent Core/Adapter 等目标边界。它是
本次事故回放的主要历史基线，但不能用来推断其它业务域也有相同结构。

## Agent Runtime 事故回放样本

Git 历史在 2026-07-19 至 2026-07-24 出现密集的跨层收束与修复，代表性 commit 包括：

- `980b5f50`：前端切换 canonical Runtime 消费链；
- `aee2b89d`：删除 Relay 旧 Prompt / Session 通道；
- `e995a82e`：接通 Workflow AgentCall Product 执行闭环；
- `ef4cc249`：接通 Product 运行期 Surface 更新；
- `9952756a`：删除 Runtime repository 与协调账本；
- `9d0e7d7c`：将 Agent 局部事实收回 owner document；
- `f234c9fc`：收口 canonical 会话单一事实链路；
- `d41593f9`：收口 Surface 代际与会话终态投影；
- `aae0ba16`：收复 ContextFrame 模型输入单一权威；
- `62424554`：修复会话重连与标题实时刷新；
- `2a4e65be`：修复 Native 会话实时状态与权威投影。

这些 commit 同时触及 Product、Workflow、Runtime、Persistence、Protocol、Frontend 等语义，能够证明
影响面很广；它们本身不能证明每一个跨模块修改都不合理。最终回放要进一步区分：

- canonical owner 变化必然造成的一次性迁移；
- 因 consumer 直接依赖内部状态或重复解释事实而产生的非必要连锁修改；
- 因 production composition / recovery / live projection 缺少闭环测试而在主重构后逐项暴露的遗漏；
- 目标边界本身在实现过程中继续变化所造成的设计漂移。

## Agent Runtime 之外的候选样本

既有任务与 review 至少暴露过以下全局候选模式，需由并行当前代码审计验证：

| 候选域 | 历史证据 | 待验证问题 |
| --- | --- | --- |
| Workflow / Lifecycle / Task | `06-14-module-overdesign-review` 曾记录 cancel 绕过 reducer、Task 由 absence 推断状态、status aggregation 重复 | 当前是否仍有多个状态推进 owner，projection 是否只读 canonical evidence |
| Permission / Capability | 同一 review 曾记录 PermissionGrant 与 companion grant 双事实链、前端镜像 capability baseline | 当前授权、availability、surface 和 UI 是否各自解释同一规则 |
| VFS / Workspace Module | 同一 review 曾记录 VFS provider 漂移成 session tool composer、mount metadata 使用 raw JSON | 当前 domain provider 是否已收窄，address/owner metadata 是否仍跨层泄漏 |
| Local / Relay / Desktop | 同一 review曾记录 local CommandHandler 全域 hub、Tauri shell 持有 profile/claim 业务 | 当前 hub 是否只是合理 router，还是持有跨域状态与 fallback |
| Extension Runtime | 05-26/05-27 系列任务反复收敛 host、workspace/process/env/schema contract | 当前 extension action、workspace handle、permission 和 local host 是否共享稳定合同 |
| Frontend state/projection | 03-25 review 与 06-14 review 曾记录页面/store 重新派生 owner、running、mailbox、mount selection | 当前 reducer/effect/store 是否仍是第二业务状态机 |

## Git 共变初筛

主会话对 2026-06-01 至当前的 non-merge commits 做了只读初筛。模块单位为 `crates/<name>`、
`packages/<name>`、`scripts`、`tests`、`deploy`、`schemas`；只统计至少触及一个这些模块的 commit。

| 样本 | Commit 数 | 触及 2+ 模块 | 模块数 P50 / P90 / P95 | 最大 |
| --- | ---: | ---: | ---: | ---: |
| 全部产品相关 commit | 1150 | 67.7% | 2 / 7 / 9 | 32 |
| 排除 subject 含 runtime/session/agentrun/compaction/contextframe/connector | 572 | 64.5% | 2 / 7 / 8 | 20 |

这说明跨模块修改不是 Agent Runtime 独有现象，但不能直接解释为 64% 的 commit 都存在坏耦合。仓库采用
Rust 多 crate、前后端 generated contract 和纵向 feature commit，API + application + contract + frontend
共同变化本来就可能合理。该数据只用于选择深挖样本：

- diagnostics 统一曾触及 20 个模块；
- agent lifecycle 事实源收束触及 14 个模块；
- Workspace Module / Canvas Interaction 替换触及 13 个模块；
- ExtensionProtocol 原子收敛触及 13 个模块；
- Tool Catalog 协议恢复触及 11 个模块；
- Extension App 生成投影闭环触及 11 个模块。

最终报告要判断这些宽 commit 中哪些属于一次稳定 contract 的原子迁移，哪些是 consumer 直接知道内部
实现而导致的非必要共变。P90/P95 更适合描述仓库的变化局部性基线，不作为风险阈值。

## 当前架构维护门禁的直接证据

当前 `Cargo.toml` workspace 已不存在 `agentdash-executor`、`agentdash-agent-types`、
`agentdash-plugin-api`、`agentdash-first-party-plugins`，但 active architecture 文档仍把它们当作当前
crate：

- `.trellis/spec/tech-stack.md:66,69,74-75`
- `.trellis/spec/backend/directory-structure.md:23,26,50,69-70`
- `.trellis/spec/cross-layer/desktop-local-runtime.md:127,183-184`
- `README.md:187`
- `README.zh-CN.md:157`

更关键的是 `scripts/check-background-process-spawn.js` 的 `scanRoots` 仍含
`crates/agentdash-executor`；`walk()` 对不存在目录直接返回空集合，因此模块删除或重命名会静默缩小
guard 覆盖面。仓库脚本、`package.json` 和 CI 配置中也没有搜索到该 guard 的调用入口。

这不是单纯文档过期：它说明当前“模块边界声明 → 可执行 guard → workspace 实际成员”之间没有 drift
闭环。最终报告应把架构索引和 guard coverage 本身视为一个需要 owner 的 contract，并要求不存在的
scan root 直接失败。

## 跨历史样本的暂定假设

以下只是待当前代码验证的研究假设：

1. **边界以“大 DTO / RepositorySet / AppState / Context object”表达时，信息隐藏很弱。**
   调用方虽然不 import 具体实现，却仍知道 owner 的全部数据形状和装配顺序。
2. **projection 同时承担查询、command availability 和恢复事实时，读模型会变成第二状态机。**
3. **同一概念同时出现在 Product、Runtime、Driver、Wire 与 UI 时，缺少明确的 canonical/source mapping
   就会形成语义耦合，而不只是必要协议映射。**
4. **只测试单 crate/service 的 happy path，无法守住 production composition、disconnect/replay、
   migration 和 renderer 闭环。**
5. **频繁“收敛”任务可能代表系统持续主动还债，也可能代表目标边界从未稳定。**
   本次必须结合 residual code 与重复事故判断，不能仅按任务数量下结论。

## 本地会话历史

使用 `trellis mem` 搜索了“模块”“耦合”“稳定边界”“Agent Runtime 重构”等关键词。命中的主要历史包括：

- session `019ec3f8-9585-7272-8ff9-de30fac8a6c6`：06-14 横向一般后端模块 review；
- session `019d257b-ce80-7401-925e-4de188fe0862`：03-25 全仓前后端架构 review；
- session `019ee831-c3c9-7d40-8d62-e2da15d5c622`：06-21 后端 crate 主链路拓扑；
- session `019ee832-16c0-72c2-bdeb-7a6bb6367273`：06-21 Local / Relay / Desktop 主链路；
- session `019f793f-f454-7b92-b156-58b625ee3cb8` 及同阶段会话：用户要求为 Runtime
  重构显式记录每一步稳定边界，避免功能路径断裂。

这些会话仅作为历史定位线索；最终问题仍以当前仓库证据为准。
