# Project 虚拟工作空间与外部 Provider 服务规划

## Goal

把 `Project` 从“只负责组织物理 workspace 的元数据容器”升级为“可挂载虚拟工作空间的编排根节点”，并进一步让 `Project / Story` 都能维护各自的“文件形态上下文容器”，使系统可以在不绑定物理目录的前提下，对接 KM、规范库、快照库、企业知识服务等非物理内容源，并以统一的 Address Space / Provider 接口向 Agent、上下文注入和前端浏览能力暴露。

## What I Already Know

- 当前 `main` 物理 workspace 已经通过统一的 Address Space service + runtime tool provider 跑通。
- 现有规范已经明确 `mount + relative path` 是统一定位模型，且 KM / Snapshot 应表现为受限 VFS。
- `Project`、`Workspace` 元数据归云端；云端不应直接访问宿主机文件系统；本机后端不应读写业务数据库。
- 当前实体关系是稳定的三层结构：
  - `Project` 负责项目级配置与默认策略；
  - `Story` 负责维护完整上下文与编排；
  - `Task` 负责绑定执行环境与具体 Agent 进程。
- 当前 `Story.context` 已经能维护声明式 `source_refs`，`Task.agent_binding` 已经能维护 task 级追加上下文来源，但它们还不是“统一的文件形态容器”。
- 当前活跃收口任务仍在处理“目录选择 / 本机路径探测”边界，说明物理目录流程还需要进一步收死。
- 用户希望：
  - 将 `Project` 模拟为一个可用的虚拟工作空间；
  - 对内置的 `Project / Story / Task` 结构给出统一方案，而不是只为某一个层级单独补丁；
  - 在 `Project` 和 `Story` 下面都能维护各自的文件形态上下文容器；
  - 它们都可以声明“派生给下级 Agent 的默认追加容器集合”，例如是否附带本地工作空间、是否只读、是否允许 `exec/write`；
  - 为其设计妥善的数据保管方案；
  - 以“跑通完整流程”为出发点反推接口；
  - 非物理工作空间最好有一套通用可扩展服务接口，方便未来接入企业 KM，而不是把企业定制逻辑写死在本项目里。

## Assumptions (Temporary)

- 第一阶段优先做“Project 级虚拟工作空间描述 + provider 接入协议 + 最小端到端链路”，不追求一次覆盖全部企业场景。
- 第一阶段虽然从 `Project` 层起步，但模型本身必须从一开始支持 `Project / Story` 两级容器。
- 虚拟工作空间的“内容数据”不默认存放在 AgentDash 主业务存储中；AgentDash 负责保存 provider 连接配置、挂载描述、访问策略和缓存元数据。
- 面向企业集成时，优先采用“外部 provider 服务 / 网关”模式，而不是在 AgentDash 云端内直接适配每一种企业 API。
- `Task` 层首轮不直接持有自己的虚拟容器定义，而是消费 `Project / Story` 派生下来的容器集合，再叠加自身 `workspace_id` 和 `agent_binding`。

## Requirements

- `Project` 和 `Story` 都需要能挂载至少一种“非物理虚拟工作空间 / 文件形态上下文容器”，并能像普通 mount 一样参与 Agent 访问与上下文注入。
- 需要定义通用的 provider 服务接口，使外部服务可声明能力并提供 `read / list / search / stat`，后续可按需扩展 `write`。
- 虚拟工作空间必须保留统一的 `mount + relative path` 定位方式，不能把企业侧接口细节暴露给 Agent。
- 需要定义“容器派生策略”：
  - `Project` 可声明默认提供给其下 `Story / Task / Session` 的容器；
  - `Story` 可在继承 `Project` 默认容器的基础上追加、屏蔽或重定权某些容器；
  - `Task` 默认消费派生结果，并与其物理 `workspace_id` / `agent_binding` 一起构成最终 session mount table。
- 需要让“是否提供本地工作空间、是否可读写、是否允许 exec、是否默认暴露给某类 agent”都进入结构化策略，而不是散落在 prompt 拼接或路由分支里。
- 需要明确数据保管边界：
  - AgentDash 云端保存什么元数据；
  - 外部 provider 自己保存什么业务数据；
  - 哪些内容允许缓存、缓存生命周期如何定义。
- 需要从“可跑通流程”倒推接口：Project / Story 配置容器 -> 会话生成 mount table -> Agent / context / browse 成功读取。
- 首轮应至少支持只读虚拟空间，且默认不支持 `exec`。
- provider 模型要允许后续接入企业级 KM 服务，而不要求在 AgentDash 内部为每个企业接口单独开发一套 adapter。

## Acceptance Criteria

- [ ] 有一份明确的 `Project / Story` 级虚拟工作空间模型，说明其与现有物理 `Workspace` 的职责边界。
- [ ] 有一份通用 provider 服务契约，覆盖注册/发现、能力声明、资源读写基础接口和错误矩阵。
- [ ] 有一份清晰的“容器派生策略”模型，说明 Project 默认容器、Story 覆盖规则、Task 最终 mount 合成逻辑。
- [ ] 明确 AgentDash 与外部 provider 的数据保管分工，并给出推荐缓存策略。
- [ ] 明确第一条最小闭环链路的 API/服务编排：Project/Story 配置 -> mount 生成 -> 读取成功。
- [ ] 明确哪些能力是首轮必须做，哪些延后。
- [ ] 方案能自然扩展到企业 KM 服务，而不是把企业定制逻辑侵入到主框架中。

## Definition of Done

- 新任务 PRD 明确 MVP 范围、接口方向、错误语义与数据归属
- 新任务 PRD 明确 owner 层级、派生规则和最终 session mount 生成逻辑
- 与现有 Address Space 规范保持一致，不重新引入另一套寻址模型
- 后续实现任务可以按小 PR 拆分，不需要再回头大幅返工

## Technical Approach

建议采用“三层编排 + 一层 provider”模型：

1. `Context Container Definition`
- 容器定义不是直接绑在 `Task` 上，而是作为 `Project / Story` 的结构化资源。
- 每个容器定义都表现为一个逻辑 mount 描述，而不是物理目录。
- 云端持久化：
  - owner（`project` / `story`）
  - provider 类型
  - provider 连接引用
  - mount id / display name
  - 能力声明
  - 访问策略与缓存配置
  - 派生策略（默认暴露对象、默认权限、是否参与 agent session）

2. `Container Exposure Policy`
- `Project` 维护项目级默认容器集合，类似“项目默认上下文文件系统”。
- `Story` 维护故事级容器集合，可在项目默认值基础上：
  - 追加新容器
  - 覆盖已有容器权限
  - 显式禁用某个上游默认容器
- `Task` 不首发持有独立容器定义，而是得到一份“派生后的最终 mount 计划”。
- 最终 session mount table 至少由三部分合成：
  - 物理工作空间 `main`（如果 Task 绑定了 workspace）
  - Project 级默认虚拟容器
  - Story 级追加/覆盖后的虚拟容器

3. `Task Session Mount Plan`
- 在真正启动 Task / Story Session 时，云端根据 owner 层级规则生成最终 mount table。
- 这一步才把“哪些容器对哪个 agent 可见、权限如何”固化下来。
- 这样可以避免把策略分散在 prompt 模板、context resolver 和 runtime tool 各自的逻辑里。

4. `External Provider Service`
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

## Aligned Intent

基于当前代码和你的补充，我理解你的意图是：

1. `Workspace` 继续只表示“物理执行环境”
- 它仍然通过 `backend_id + container_ref` 指向真实目录。
- 它的职责是代码执行、本地文件读写、`shell.exec`。

2. `Project / Story` 需要新增“文件形态上下文容器”能力
- 它们不是执行目录，而是“可挂载的上下文文件系统”。
- 它们既可以承载内置数据结构导出的文件视图，也可以承载外部 provider 返回的虚拟文件。

3. 容器需要能参与“派生”
- `Project` 规定项目级默认追加容器；
- `Story` 在项目默认值基础上细化；
- `Task` 启动 agent 时吃到最终结果，而不是自己临时拼接。

4. 派生结果不仅决定“挂哪些容器”，还决定“给多大权限”
- 是否附带本地工作空间
- 虚拟容器是否只读
- 是否允许 `write / exec / search`
- 是否只对某些 agent 类型默认暴露

5. 对内置数据结构与外部企业服务，底层都走同一套 provider 抽象
- 内置的 `Project / Story` 文件化视图不应该是特判路径；
- 企业 KM 也不应该侵入主框架；
- 两者都应该只是不同 provider 的实现来源。

如果按这个意图推进，那么“虚拟工作空间”更准确的名字其实接近：
- `Context Container`
- `Virtual Mount`
- `Derived Mount Policy`

它不只是给 `Project` 一个虚拟 workspace，而是给整条 `Project -> Story -> Task` 链引入统一的“上下文容器编排”能力。

## Decision (ADR-lite)

**Context**: 统一 Address Space 底座已经完成第一轮落地，下一步需要让非物理空间真正进入产品主链路，同时避免把企业接口定制写进核心框架。
**Decision**: 采用“`Project / Story` 级上下文容器 + 派生挂载策略 + 外部 provider 服务协议”的方向，AgentDash 负责挂载、授权、缓存和编排，企业数据接入由外部 provider 服务承接。
**Consequences**:
- 优点：主框架保持稳定，企业接入成本更低，Address Space 统一模型得到复用。
- 优点：`Story.context`、`Task.agent_binding.context_sources` 和未来虚拟 workspace 不再是三套松散机制，而是逐步并到同一个容器编排模型。
- 代价：需要新增 provider 注册/鉴权/缓存策略设计，也需要补一层 owner 级派生规则，且首轮实现要控制在只读 MVP。

## Out of Scope

- 首轮不承诺完整可写虚拟文件系统
- 首轮不承诺企业权限体系的深度映射
- 首轮不把 embedding / RAG / 索引构建全部做完
- 首轮不替代现有物理 workspace 流程
- 首轮不让 Task 自定义新的容器定义模型，Task 仅消费派生结果

## Technical Notes

- 相关规范：
  - `.trellis/spec/backend/address-space-access.md`
- 相关现状：
  - `crates/agentdash-api/src/address_space_access.rs`
  - `crates/agentdash-api/src/task_agent_context.rs`
  - `crates/agentdash-api/src/routes/address_spaces.rs`
  - `crates/agentdash-executor/src/connectors/pi_agent.rs`
  - `crates/agentdash-domain/src/project/entity.rs`
  - `crates/agentdash-domain/src/project/value_objects.rs`
  - `crates/agentdash-domain/src/story/entity.rs`
  - `crates/agentdash-domain/src/story/value_objects.rs`
  - `crates/agentdash-domain/src/task/entity.rs`
  - `crates/agentdash-domain/src/task/value_objects.rs`
- 需要和 `03-18-local-directory-capability-closure` 保持边界清晰：
  - 物理目录能力继续收口到 backend
  - 虚拟工作空间不依赖本机目录选择器
- 推荐的 MVP 闭环：
  - Project 配置一个项目级 `spec` 虚拟容器
  - Story 追加一个故事级 `brief`/`km` 容器
  - Session mount table 按派生规则生成最终 mount
  - `fs.read` 或 declared source 可以成功读取 provider 返回内容
