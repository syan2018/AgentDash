# Local Runtime无数据库Host状态重建

## Goal

让正式Tauri Desktop Local Runtime与Standalone Runner成为无数据库的本机执行包：启动时从文本配置、Integration definitions和云端canonical Runtime重建Host状态，不再启动embedded PostgreSQL、执行Dashboard migrations或承担Local DB发布升级生命周期。

## Background

- 当前`agentdash-local::build_ws_config`无条件启动按backend分区的embedded PostgreSQL、执行完整Dashboard migration集合，再以`PostgresAgentRuntimeHostRepository`初始化Local Integration Driver Host。
- Local Host实际使用的持久事实集中在service instance/revision、activation、offer、binding、driver lease与source/driver coordinate；workspace、MCP、machine identity、runner profile和credential refs已经由文本配置或云端事实提供。
- Local bootstrap当前没有调用durable pending-binding recovery；数据库承担完整schema与发布成本，却没有形成完整的跨进程恢复闭环。
- Tauri updater与Standalone Runner发布链均没有主动、可控的Local DB migration阶段。把migration隐藏在首次Runtime start中会让无关Dashboard schema阻断Backend上线。

## Requirements

### R1. Local Host状态归属

- Local Host的service instance、offer、binding、lease与coordinate均为单个Host incarnation内的执行状态。
- Project、AgentRun、Runtime thread/binding intent和恢复裁决继续由云端Managed Runtime与AgentRun facade拥有。
- 本机workspace、MCP、machine identity、profile与credential reference继续读取现有文本或系统事实源，不新增第二持久化格式。

### R2. Ephemeral Host Repository

- 提供正式production-grade ephemeral `AgentRuntimeHostRepository`实现，复用当前repository invariants，但不得直接以测试`Fixture`命名或依赖测试支持层。
- `agentdash-local`启动时重建Integration definitions、service instances与offers；进程退出后所有pending/active binding、lease和coordinate自然失效。
- Tauri embedded runner、Web dev local runtime与Standalone Runner共用同一无数据库bootstrap。

### R3. Host Incarnation与旧命令隔离

- 每次Local Host进程启动建立新的不可复用incarnation identity。
- offer、binding、dispatch与lease admission必须能证明属于当前incarnation；旧连接、旧generation或旧binding command不能在重启后复活。
- Backend断连时云端将旧Host bindings收敛为Lost；重连后通过canonical binding/resume intent重新建立Driver binding，不从本机恢复旧binding。
- generation fencing不能依赖本机数据库单调计数；由incarnation或云端分配的connection-scoped generation保证跨重启隔离。

### R4. 移除Local PostgreSQL启动链

- `agentdash-local`不再调用`PostgresRuntime::resolve_embedded_at_data_root`或全局`run_postgres_migrations`。
- Local Runtime启动、credential claim、relay registration与Driver Host availability不再依赖Dashboard schema。
- 移除仅为Local Host PostgreSQL存在的runtime handle、crate依赖和测试fixture；既有本机数据库不参与运行时读取或兼容迁移。

### R5. 可诊断恢复

- Local Host重启后重新广告offers，并让云端明确区分“新incarnation已上线”和“旧binding已丢失”。
- 活跃AgentRun必须重新bind/resume或进入归属明确的Lost状态；不能静默假装旧Driver仍然可用。
- 日志与runtime health展示Host incarnation、offer generation和重绑定结果，但不记录credential或业务输入。

## Acceptance Criteria

- [ ] Tauri Desktop Local Runtime和Standalone Runner启动时不创建PostgreSQL进程、数据目录或`_sqlx_migrations`。
- [ ] `agentdash-local`不依赖完整Dashboard migration集合，修改Dashboard schema不会影响本机Backend上线。
- [ ] Local Integration definitions/service instances/offers可从现有配置在空内存状态下重建并完成relay注册。
- [ ] Local Host进程重启产生新incarnation；旧binding、lease、coordinate和延迟command均被拒绝。
- [ ] 云端在Backend断连后将旧binding收敛Lost，并能以canonical source thread/resume intent建立新binding。
- [ ] 真实`pnpm dev`、`pnpm dev:desktop`以及Standalone Runner前台/service路径均验证无数据库启动和Backend online。
- [ ] 删除本机Runtime数据库目录后产品能力无损；旧目录存在时运行时不读取、不迁移且不影响启动。
- [ ] 相关Host、Runtime Wire、relay、Tauri与Runner测试覆盖首次启动、重启、断连重绑和旧命令隔离。

## Out of Scope

- 云端Dashboard PostgreSQL与Managed Runtime durable store不在本任务移除范围。
- 不为旧Local PostgreSQL内容提供兼容读取、数据迁移或双写。
- Runner自动更新与Desktop API sidecar migration属于各自发布任务；本任务通过移除Local DB消除本机Host migration需求。
