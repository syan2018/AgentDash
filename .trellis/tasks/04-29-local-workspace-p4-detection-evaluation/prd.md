# brainstorm: 评估 local 模块 workspace 识别与 P4 扩展

## Goal

评估 `agentdash-local` 及其上游 workspace detection 链路，对“工作空间实际是什么”这件事目前到底能识别到什么程度、能把哪些事实稳定标记到 `Workspace.identity_*` 和 `WorkspaceBinding.detected_facts` 中，尤其明确 `P4Workspace` 目前为什么还没有真正落地，并给出后续扩展开发的推荐切入顺序。

## What I already know

* 逻辑工作空间模型已经允许三种身份：`GitRepo`、`P4Workspace`、`LocalDir`，但这只是领域枚举，不代表探测链路已实现对应能力。
* 当前对远程目录的 workspace 自动识别由云端 application 层 `detect_workspace_from_backend()` 驱动，而不是由 `agentdash-local` 独立完成完整识别。
* `agentdash-local` 当前暴露的相关能力只有 `workspace_detect_git`，且实现仅做 `root/.git` 是否存在的布尔判断。
* 当本机探测返回“不是 Git”时，云端会直接把目录识别成 `LocalDir`，并明确附带 warning：`P4 自动识别尚未接入`。
* 即便识别成 Git，本机也没有回传 `remote_url / branch / commit_hash`，因此云端会把 `remote_url` 退化为当前 `root_ref`，这会让相同仓库的不同 clone 看起来像不同 logical workspace。
* `detect_workspace` 接口目前通过 `identity_kind + identity_payload` 的完全相等来匹配已有 Workspace，因此 identity 一旦退化，后续“识别已有 workspace / 追加 binding”的能力也会跟着失真。

## Current Assessment

### 1. 当前实际识别能力

当前链路更接近“Git presence probe + local_dir fallback”，而不是通用的 workspace identity detection：

1. API 层接收 `backend_id + root_ref`
2. application 层调用 `BackendTransport.detect_git_repo()`
3. relay 把请求转给 `agentdash-local`
4. `agentdash-local` 只校验路径是否在 `accessible_roots` 内，然后判断 `.git` 是否存在
5. 云端据此二分：
   * Git -> `identity_kind = GitRepo`
   * 非 Git -> `identity_kind = LocalDir`

### 2. 当前实际标记能力

目前能稳定写入的标记信息非常有限：

* `Workspace.identity_kind`
  只可能被自动识别成 `GitRepo` 或 `LocalDir`
* `Workspace.identity_payload`
  Git 路径下只有 `remote_url / branch / root_hint` 这几个字段位被预留，但真实值基本拿不到
* `WorkspaceBinding.detected_facts`
  仅缓存一层 `git.is_repo / source_repo / branch / commit_hash`

这意味着“标记能力”现在主要还是为 Git 留了结构，为 P4 只留了类型位，没有事实采集闭环。

### 3. P4 当前缺口

`P4Workspace` 目前只存在于：

* 领域枚举
* PostgreSQL 持久化映射
* 早期 PRD / 设计文档

但不存在于：

* local backend 的探测命令集
* `BackendTransport` 抽象
* workspace detection result 结构
* `identity_payload` 规范化策略
* `detected_facts` 的 P4 字段约定
* 自动匹配已有 workspace 的归一化规则

换句话说，当前不是“P4 识别效果不好”，而是“P4 自动识别这条能力链尚未真正接入”。

### 4. 比 P4 更早需要修的基础问题

在接 P4 之前，Git identity 本身也还不够可靠：

* local backend 不读取 Git remote
* local backend 不读取当前 branch
* local backend 不读取 commit hash
* cloud adapter 会把缺失的 `remote_url` 退化成 `root_ref`

这会让 logical workspace 的 identity 从“仓库身份”退化成“某台机器上的某个目录”，与当前领域模型想表达的“逻辑工作空间”不一致。

### 5. 识别策略还缺少“人为声明层”

当前方案仍然以“自动探测结果直接推导 logical identity”为主，但真实团队场景里，工作空间身份往往还需要人为补充：

* 这是“谁的”工作空间
* 这台机器/这个 binding 属于哪个组
* 这个 workspace 应按 `stream` 对齐，而不是按 `client_name` 对齐
* 这个 Git clone 应按 `repo + branch` 识别，还是只按 `repo` 识别

如果没有一层显式策略/标签，自动探测只能给出 facts，无法可靠决定“业务上应如何认定这是同一个工作空间”。

## Assumptions (temporary)

* 本任务以“预研与扩展设计”为目标，不要求本轮直接交付完整 P4 能力。
* 当前项目不需要兼容旧接口/旧字段，优先让模型和探测语义变正确。
* 未来 P4 workspace 的 identity 应该以“可跨 backend 识别同一工作空间”的稳定键为核心，而不是简单用绝对路径冒充身份。
* P4 探测允许依赖本机命令行或已知目录标志，但必须先定义事实字段与失败语义，再决定具体探测手段。
* 后续应允许 local 侧或云端对 workspace/binding 补充人工标签与识别策略，而不是把所有判断都硬编码在自动探测器里。

## Open Questions

* P4 logical identity 的主键应优先采用哪组事实：
  `client name`、`server + client`、`stream`，还是“仓库/映射根”的组合？
* 本项目是否接受 local backend 依赖 `p4` CLI 作为首版实现，还是需要先设计纯文件启发式探测？
* 对于 Git / P4 / Local 三类 identity，是否要先引入统一的“规范化 identity payload”层，避免 API 直接拿探测原始 JSON 做等值匹配？
* 手动标签应该挂在哪一层更合理：`Workspace`（逻辑身份策略）、`WorkspaceBinding`（物理实例属性），还是两者都需要？
* “启动前 prepare 工作空间”应是逻辑 workspace 的策略，还是具体 binding 的策略？

## Requirements (evolving)

* 明确当前 workspace detection 全链路的职责分层：local 负责采集事实，application 负责归类与置信度，API 负责匹配已有 workspace。
* 明确 `agentdash-local` 当前真正支持的识别和标记字段，避免把枚举存在误判为能力已落地。
* 输出 `P4Workspace` 从“枚举占位”走到“可用识别能力”至少需要补齐哪些接口、数据结构、事实字段和测试。
* 把 Git 与 P4 放在同一识别框架里考虑，避免再出现“Git 走一套布尔探测，P4 以后再补另一套临时分支”的结构漂移。
* 给出推荐的最小实现路径，能先改善 identity 质量，再增量接入 P4，而不是一次性做大而散的重构。
* 识别策略必须支持“自动探测 + 手动标签 + 规则组合”的模式，而不是只能靠单一路径自动识别。
* 运行前必须存在一层“verify/prepare workspace”机制，用于确认当前 binding 仍满足目标 workspace contract，并按策略把它调整到可用状态。

## Acceptance Criteria (evolving)

* [ ] 能明确说明 `agentdash-local` 当前对 workspace 的识别边界，以及哪些能力不在本机侧。
* [ ] 能明确说明为什么 `P4Workspace` 目前并未真正接入自动识别链路。
* [ ] 能列出接入 P4 所需的最小接口增量、数据模型增量和测试面。
* [ ] 能指出当前 Git identity 退化问题，并说明它为什么会影响 logical workspace 匹配。
* [ ] 能给出一个分阶段开发方案，包含“先修基础识别质量”与“再接 P4”的顺序。
* [ ] 能说明如何把“手动打标”纳入统一识别模型，而不是另起一套旁路配置。
* [ ] 能说明运行前 workspace verify / prepare 应放在哪层，以及为什么。

## Research Notes

### 代码中已经存在的事实

**本机探测只实现了 Git existence probe**

* `crates/agentdash-local/src/command_handler.rs`
  `handle_workspace_detect_git()` 只检查 `workspace_root.join(\".git\").exists()`，其余字段全部返回 `None`。

**云端 detection 目前只有 Git / Local 二分**

* `crates/agentdash-application/src/workspace/detection.rs`
  若 `git.is_git_repo == true`，返回 `WorkspaceIdentityKind::GitRepo`；
  否则直接返回 `WorkspaceIdentityKind::LocalDir`，并附加 warning：`P4 自动识别尚未接入`。

**transport 抽象只定义了 Git 探测**

* `crates/agentdash-application/src/backend_transport.rs`
  `BackendTransport` 当前只有 `detect_git_repo()`，没有更通用的 `detect_workspace()` 或 `collect_workspace_facts()`。

**Git identity 在 API adapter 层已经发生退化**

* `crates/agentdash-api/src/workspace_resolution.rs`
  当本机未提供 `remote_url` 时，`source_repo` 会回退为 `root.to_string()`。

**已有 workspace 的自动匹配依赖原样 JSON 相等**

* `crates/agentdash-api/src/routes/workspaces.rs`
  `matched_workspace_ids` 按 `identity_kind == detected.identity_kind && identity_payload == detected.identity_payload` 判断。

### 约束与启发

* 统一 Address Space 规范强调：当功能涉及云端/本机文件访问、多 workspace、上下文注入与 runtime tool 时，应优先定义统一 provider / capability / mount 边界，而不是再长新链路。
* 因此 P4 扩展最好不要以“再加一个 `workspace_detect_p4` 特例命令”收尾，而应把 transport 与 detection 结果升级成能表达多种 workspace identity 的通用结构。

### Feasible Approaches Here

**Approach A: 先抽象通用 workspace facts，再把 Git/P4 都挂上去**（推荐）

* How it works:
  local backend 新增通用探测命令或通用结果结构，统一返回“采集到的事实 + 候选 identity + warnings”；application 负责归一化为 `identity_kind/payload`。
* Pros:
  能一次解决 Git 退化和 P4 缺位；模型清晰；后续可继续接入更多工作空间类型。
* Cons:
  需要改 transport 协议、DTO、测试，改动面略大。

**Approach A+：通用 facts + 人工标签 + 组合规则**（更推荐）

* How it works:
  local backend 负责产出事实（Git/P4/目录/环境标签）；云端保存逻辑 workspace 的识别 contract，并允许 binding 追加人工标签；匹配时按 contract 选择要比较的字段组合。
* Pros:
  能支持“按 stream 对齐”“按 repo+branch 对齐”“按组/归属过滤 binding”“未来新增新型 workspace”。
* Cons:
  需要定义 contract schema、matcher 和 prepare profile，设计成本更高。

**Approach B: 保持 Git 旧链路，旁路补一个 P4 专用探测**

* How it works:
  在 local/backend/application 侧平行增加 `detect_p4_workspace`，由 API 先试 Git 再试 P4。
* Pros:
  上线快，改动较小。
* Cons:
  会把 detection 继续做成分叉逻辑；Git 的 identity 退化问题仍然单独存在；长期维护成本高。

**Approach C: 只做手工录入 P4 identity，不做自动识别**

* How it works:
  允许前端/API 显式创建 `P4Workspace`，local backend 不参与探测。
* Pros:
  最省实现量。
* Cons:
  不能解决“实际识别和标记能力”问题，只是绕开问题；与当前快捷入口方向不一致。

## Recommended Direction

建议把这项扩展拆成三个连续阶段，而不是直接“做 P4 识别”：

### Phase 1: 建立可组合的 workspace 识别模型

* 把“探测结果”和“逻辑身份 contract”分离：
  * `detected_facts`: 自动采集到的事实
  * `identity_contract`: 这个 workspace 业务上要求按什么字段对齐
  * `binding_labels`: 手动打标，如 owner/group/machine role
* 为 Git / P4 / Local 定义可组合的 identity contract，而不是把整个 payload 当主键
* matcher 不再依赖 JSON 全等，而是按 contract 逐项比较

### Phase 2: 修正现有 Git / P4 facts 采集质量

* 把 local backend 的 Git 探测从“只看 `.git`”升级为返回：
  * `repo_root`
  * `remote_url`
  * `current_branch`
  * `default_branch`
  * `commit_hash`
* 在 local backend 接入 P4 facts 采集：
  * `server_address`
  * `client_name`
  * `stream`
  * `workspace_root`
  * `user_name`
* 禁止 cloud adapter 再把 `root_ref` 冒充 `source_repo`

### Phase 3: 补齐运行前 verify / prepare

* 在 session/task 启动前，对候选 binding 做一次轻量 re-probe
* 根据 workspace contract 校验：
  * Git 是否命中目标 repo / branch / commit 约束
  * P4 是否命中目标 server / stream / client 约束
* 若允许自动修正，则按 prepare profile 执行：
  * Git: `fetch` / `switch branch` / `fast-forward` / `hard reset` / `pin commit`
  * P4: `set client` / `sync head` / `sync changelist`
* 把 prepare 结果写回 `detected_facts`，并在前端/API 暴露诊断

## Technical Approach

### 建议补齐的数据结构

可考虑引入统一探测结果，例如：

```rust
struct WorkspaceFacts {
    git: Option<GitFacts>,
    p4: Option<P4Facts>,
    markers: Vec<String>,
}

struct WorkspaceIdentityCandidate {
    kind: WorkspaceIdentityKind,
    payload: serde_json::Value,
    confidence: DetectionConfidence,
    reason: String,
}
```

以及在逻辑层定义识别 contract，例如：

```rust
struct WorkspaceIdentityContract {
    kind: WorkspaceIdentityKind,
    match_mode: String,          // git: repo_only/repo_branch/repo_commit
                                // p4: server_stream/server_client/server_stream_client
    required_fields: serde_json::Value,
    binding_label_selectors: serde_json::Value,
    prepare_profile: Option<WorkspacePrepareProfile>,
}
```

其中 `P4Facts` 首轮至少可预留：

* `client_name`
* `server_address`
* `stream`
* `root`
* `view_hash` 或其他可稳定比较的映射摘要

### 手动打标建议

建议允许在 `WorkspaceBinding` 层补充人工标签，例如：

* `owner = yihao.liao`
* `group = abc-client`
* `machine_role = primary-dev`
* `workspace_class = personal | shared | ci`

逻辑 workspace 本身则保存“允许哪些 binding 参与匹配/执行”的 selector，而不是把这些标签直接混进 identity 主键。

### prepare 落点建议

prepare 不应在 backend 刚连接时自动跑，因为那时系统还不知道“你到底要对齐哪个逻辑 workspace”。

更合理的落点是：

* `task/service` / `acp_sessions` 解析出目标 workspace 和 binding 之后
* `session_hub.start_prompt()` 之前

也就是：**先 resolve binding → verify contract → prepare if needed → 再启动 session**。

### 建议测试面

* local backend 单测：
  * Git 目录返回完整 facts
  * 非 Git / 非 P4 返回 local_dir
  * P4 根目录返回候选 identity 或明确 unsupported warning
* application 单测：
  * Git facts -> normalized git identity
  * P4 facts -> normalized p4 identity
  * 多候选 identity 的置信度与 warning 传递
* API 单测：
  * 同 identity 的不同 binding 能匹配到同一个 workspace
  * identity 不完整时返回明确未匹配原因

## Definition of Done (team quality bar)

* 结论与方案已记录到 task PRD，可作为后续实现任务的输入
* 已明确列出当前能力、缺口、阶段方案和风险
* 若后续启动实现，可直接按本 PRD 拆分子任务

## Out of Scope (explicit)

* 本轮直接实现完整 P4 自动识别
* 为旧 API / 旧 identity_payload 形态保留兼容路径
* 一次性重写整个 VFS / Address Space 系统

## Technical Notes

* 关键代码入口：
  * `crates/agentdash-local/src/command_handler.rs`
  * `crates/agentdash-application/src/workspace/detection.rs`
  * `crates/agentdash-api/src/workspace_resolution.rs`
  * `crates/agentdash-api/src/routes/workspaces.rs`
  * `crates/agentdash-domain/src/workspace/value_objects.rs`
* 相关历史任务：
  * `03-24-project-workspace-backend-refactor`
  * `03-18-local-directory-capability-closure`
  * `03-25-relay-address-space-boundary-hardening`
* 当前判断：
  * `P4Workspace` 是“领域模型已声明、探测链路未接入”的状态
  * 在修 P4 之前，应该先纠正 Git identity 的事实采集与归一化
