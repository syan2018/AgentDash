# Project 虚拟工作空间与外部 Provider 服务规划

## Goal

把 `Project` 从“只负责组织物理 workspace 的元数据容器”升级为“可挂载虚拟工作空间的编排根节点”，让项目能够在不绑定物理目录的前提下，对接 KM、规范库、快照库、企业知识服务等非物理内容源，并以统一的 Address Space / Provider 接口向 Agent、上下文注入和前端浏览能力暴露。

## What I Already Know

- 当前 `main` 物理 workspace 已经通过统一的 Address Space service + runtime tool provider 跑通。
- 现有规范已经明确 `mount + relative path` 是统一定位模型，且 KM / Snapshot 应表现为受限 VFS。
- `Project`、`Workspace` 元数据归云端；云端不应直接访问宿主机文件系统；本机后端不应读写业务数据库。
- 当前活跃收口任务仍在处理“目录选择 / 本机路径探测”边界，说明物理目录流程还需要进一步收死。
- 用户希望：
  - 将 `Project` 模拟为一个可用的虚拟工作空间；
  - 为其设计妥善的数据保管方案；
  - 以“跑通完整流程”为出发点反推接口；
  - 非物理工作空间最好有一套通用可扩展服务接口，方便未来接入企业 KM，而不是把企业定制逻辑写死在本项目里。

## Assumptions (Temporary)

- 第一阶段优先做“Project 级虚拟工作空间描述 + provider 接入协议 + 最小端到端链路”，不追求一次覆盖全部企业场景。
- 虚拟工作空间的“内容数据”不默认存放在 AgentDash 主业务存储中；AgentDash 负责保存 provider 连接配置、挂载描述、访问策略和缓存元数据。
- 面向企业集成时，优先采用“外部 provider 服务 / 网关”模式，而不是在 AgentDash 云端内直接适配每一种企业 API。

## Requirements

- `Project` 需要能挂载至少一种“非物理虚拟工作空间”，并能像普通 mount 一样参与 Agent 访问与上下文注入。
- 需要定义通用的 provider 服务接口，使外部服务可声明能力并提供 `read / list / search / stat`，后续可按需扩展 `write`。
- 虚拟工作空间必须保留统一的 `mount + relative path` 定位方式，不能把企业侧接口细节暴露给 Agent。
- 需要明确数据保管边界：
  - AgentDash 云端保存什么元数据；
  - 外部 provider 自己保存什么业务数据；
  - 哪些内容允许缓存、缓存生命周期如何定义。
- 需要从“可跑通流程”倒推接口：Project 配置 provider -> 会话生成 mount -> Agent / context / browse 成功读取。
- 首轮应至少支持只读虚拟空间，且默认不支持 `exec`。
- provider 模型要允许后续接入企业级 KM 服务，而不要求在 AgentDash 内部为每个企业接口单独开发一套 adapter。

## Acceptance Criteria

- [ ] 有一份明确的 Project 级虚拟工作空间模型，说明其与现有 `Workspace` 的职责边界。
- [ ] 有一份通用 provider 服务契约，覆盖注册/发现、能力声明、资源读写基础接口和错误矩阵。
- [ ] 明确 AgentDash 与外部 provider 的数据保管分工，并给出推荐缓存策略。
- [ ] 明确第一条最小闭环链路的 API/服务编排：Project 配置 -> mount 生成 -> 读取成功。
- [ ] 明确哪些能力是首轮必须做，哪些延后。
- [ ] 方案能自然扩展到企业 KM 服务，而不是把企业定制逻辑侵入到主框架中。

## Definition of Done

- 新任务 PRD 明确 MVP 范围、接口方向、错误语义与数据归属
- 与现有 Address Space 规范保持一致，不重新引入另一套寻址模型
- 后续实现任务可以按小 PR 拆分，不需要再回头大幅返工

## Technical Approach

建议采用“两层模型”：

1. `Project Virtual Workspace`
- 挂在 `Project` 下面，表现为一个逻辑 mount 描述，而不是物理目录。
- 云端持久化：
  - provider 类型
  - provider 连接引用
  - mount id / display name
  - 能力声明
  - 访问策略与缓存配置

2. `External Provider Service`
- 作为独立可部署服务或网关，对接企业 KM / 文档系统 / 资源库。
- 对 AgentDash 暴露稳定统一的 provider 协议，而不是把企业 API 细节塞进 AgentDash。
- AgentDash 云端通过 provider client 调用它，并把返回内容映射进 Address Space。

第一阶段建议先做只读 provider：
- `list`
- `read`
- `search`
- `stat`

后续再评估：
- `write`
- 增量同步
- 权限透传
- embedding / 召回增强

## Decision (ADR-lite)

**Context**: 统一 Address Space 底座已经完成第一轮落地，下一步需要让非物理空间真正进入产品主链路，同时避免把企业接口定制写进核心框架。  
**Decision**: 采用“Project 级虚拟工作空间 + 外部 provider 服务协议”的方向，AgentDash 负责挂载、授权、缓存和编排，企业数据接入由外部 provider 服务承接。  
**Consequences**:
- 优点：主框架保持稳定，企业接入成本更低，Address Space 统一模型得到复用。
- 代价：需要新增 provider 注册/鉴权/缓存策略设计，且首轮实现要控制在只读 MVP。

## Out of Scope

- 首轮不承诺完整可写虚拟文件系统
- 首轮不承诺企业权限体系的深度映射
- 首轮不把 embedding / RAG / 索引构建全部做完
- 首轮不替代现有物理 workspace 流程

## Technical Notes

- 相关规范：
  - `.trellis/spec/backend/address-space-access.md`
- 相关现状：
  - `crates/agentdash-api/src/address_space_access.rs`
  - `crates/agentdash-api/src/task_agent_context.rs`
  - `crates/agentdash-api/src/routes/address_spaces.rs`
  - `crates/agentdash-executor/src/connectors/pi_agent.rs`
- 需要和 `03-18-local-directory-capability-closure` 保持边界清晰：
  - 物理目录能力继续收口到 backend
  - 虚拟工作空间不依赖本机目录选择器
- 推荐的 MVP 闭环：
  - Project 配置一个 `spec`/`km` 虚拟挂载
  - Session mount table 生成对应 mount
  - `fs.read` 或 declared source 可以成功读取 provider 返回内容
