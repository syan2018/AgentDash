# Runtime 集成冲突根因与模块独立性审计

## 1. 结论

冲突面大，主要不是 WorkspaceModule、Channel、Interaction、Operation 的核心领域彼此耦合，而是这些
能力在旧架构中同时穿过少数 Runtime 共享瓶颈。更准确的判断是：**业务核心相对独立，旧集成边界高度
集中，并且 Runtime identity 与内部对象泄漏到了不应感知它们的模块。**

55 个显式冲突可从 Git mechanics 角度分解为：

- 24 个 modify/delete（43.6%）：双方以不同顺序清理同一批旧入口；
- 12 个 spec/journal content conflict（21.8%）：协作记录，不是产品代码耦合；
- 19 个产品代码 content conflict（34.5%）：集中在 API composition、AgentRun surface、frame
  construction、公共 exports 与少数 integration adapters。

PR 新增的 Managed Runtime crates 与当前分支新增的 Operation core、Interaction domain 基本没有直接
路径冲突。因此本次不能把 55 个路径都当成领域边界失败，也不能因为多数核心目录可直接搬运就忽略
19 个真实产品接缝暴露的结构问题。

## 2. 根因分类

| 根因 | 代表证据 | 性质 | 迁移中的目标处置 |
| --- | --- | --- | --- |
| 对同一旧实现并行 cutover | PR 删除 RuntimeSession、旧 mailbox/surface；当前分支删除旧 Canvas route、Extension action、WorkspaceModule runtime 文件 | Git 拓扑与迁移顺序问题 | 以 PR 最终删除为基线，只提取当前分支业务不变量，不重放旧文件上的中间实现 |
| Runtime identity 扩散为通用上下文 | `AgentRunExecutionRef`、`delivery_runtime_session_id` 进入 SPI、MCP、VFS、WorkspaceModule 与 tool context | 不当实现泄漏 | RuntimeThread/Binding 只存在于 AgentRun facade、Managed Runtime、Tool Broker 和专用 adapter；业务模块使用 `run_id + agent_id`、principal、scope |
| AgentFrame 同时承担业务配置、Runtime surface 和领域专用投影 | `agent_run/frame/**`、`runtime_surface_update.rs`、`frame_construction/**` 与 lineage repository 同时被双方修改 | 必要汇聚点承担了过多模块知识 | 各模块提供 protocol-neutral capability/operation descriptor；Surface compiler 生成 immutable snapshot，AgentFrame 只保存稳定业务引用与 revision |
| 两种 Operation 的 ownership 不够明确 | application Operation gateway 与 Runtime Operation/Tool Broker 都接近 tool admission/execution | 必要调用链存在权责重叠风险 | Tool Broker 只负责 Runtime Item 接受、恢复与 terminal；Operation Gateway 只负责 exact provider、授权、placement 与业务调用；由一个 trusted executor adapter 串联 |
| Channel / WorkspaceModule 直接构造 AgentRun 内部对象 | Channel 构造 mailbox message；旧 WorkspaceModule 同时了解 RuntimeSession anchor、AgentRun bridge、VFS 与 gateway | 不当跨层依赖 | Channel 只提交产品级 delivery intent；WorkspaceModule 只产出 projection/contribution；adapter 解析 mailbox、thread 与 broker 坐标 |
| Composition root 过于扁平 | `app_state.rs`、`bootstrap/repositories.rs`、`integrations.rs`、`application/lib.rs`、`spi/lib.rs` | 顶层装配必要，但低层对象与业务分支集中放大冲突 | 分为 Runtime、Operation、Interaction、Channel composition handle；AppState 只持有 facade/handle，feature-local registrar 负责具体 provider/repository |
| 全局串行资产造成伪耦合 | migration 0061/0062 重号、`generate_ts.rs`、contract/spec indexes、journal | 必要协调与生成热点 | migration 顺排 0061–0067；领域独立注册 contract，顶层只聚合；generated artifacts 从最终 source 重建 |
| 长生命周期分支暴露全部中间状态 | 同一 merge-base 上 73 与 11 个独有提交，无 patch-equivalent commit | 集成节奏问题 | 本次从 PR 最终态按主题搬运；每个主题形成独立 checkpoint，后续架构分支在计划删除共享 seam 前尽早集成 |

## 3. 最终允许存在的集成面

```text
AgentRun product coordinate
  -> AgentRunRuntime facade
  -> RuntimeThread / Binding

AgentFrame business facts
  -> Operation/Capability contribution compiler
  -> AgentSurfaceSnapshot
  -> Driver Host / Tool Broker

Runtime ToolCall
  -> Tool Broker
  -> trusted Operation executor
  -> exact MCP / Extension / Interaction provider

Channel ingress
  -> ChannelAgentDeliveryPort
  -> AgentRun mailbox/facade
```

这些接缝是产品行为要求决定的，不需要消灭。独立性的含义是：接缝由稳定 port 表达，并由单独 adapter
吸收 Runtime 变化；不是让模块之间完全没有调用关系。

## 4. Before / After 验收指标

### 4.1 逐路径冲突审计

对 95 个重叠路径维护 ledger，记录双方意图、根因类别、canonical owner、搬运/删除/重接结论、最终
依赖边与验证证据。55 个显式冲突和 40 个自动合并路径都必须关闭；自动合并不视为语义正确证据。

### 4.2 Cargo 依赖方向

- Channel/Interaction domain 不依赖任何 `agentdash-agent-runtime*` crate；
- Operation core 不依赖 AgentRun、Driver Host 或 RuntimeWire；
- Managed Runtime 不依赖 WorkspaceModule、Extension 或 Channel 实现；
- WorkspaceModule core 不依赖 RuntimeSession 或 AgentRun mailbox。

Runtime adapter 可以同时依赖两侧，并成为唯一反腐层。

### 4.3 Runtime identity 扩散范围

`RuntimeThreadId`、`AgentRunRuntimeBinding` 只允许出现在 Runtime contract、AgentRun facade、Tool Broker、
infrastructure adapter 与 API composition allowlist。以下旧 identity 在产品代码中应为零：

```text
AgentRunExecutionRef
delivery_runtime_session_id
agentdash_application_runtime_session
```

Channel、Interaction domain、Operation core 和 WorkspaceModule projection 不出现 RuntimeThread identity。

### 4.4 Composition fan-out

AppState 主要持有领域 facade/composition handle，不逐个持有 Runtime repository、worker、driver port 和
业务 provider。新增 Interaction/Channel provider 时，顶层仅增加或替换一个领域 handle，不产生多处
repository/transport 字段修改。

### 4.5 独立测试能力

- Operation core 使用 fake provider 独立验证 authorization、re-admission、cancel 与 result；
- Channel 使用 fake delivery port 验证 admission/delivery；
- WorkspaceModule 使用 fake Operation/contribution port 验证 catalog/presentation；
- Runtime kernel 在不存在 Extension、Interaction、Channel 实现时完成状态机测试。

这些测试不构造完整 AppState、Driver Host 或 RuntimeSession。

### 4.6 变更放大演练

最终结构至少通过以下 reasoning/diff review：

1. RuntimeThread 字段变化不修改 Channel/Interaction domain；
2. WorkspaceModule contribution 变化不修改 Managed Runtime kernel；
3. Native/Codex Driver 替换不修改 application Operation provider。

每种变化应局限于 owning module、一个 adapter 与少量 composition/tests。若再次扩散到大量跨域文件，
说明 port 仍暴露了具体实现而不是稳定业务坐标。

## 5. 最终判断标准

共享文件数量不必为零。成功标准是：共享文件只承担显式组合和生成注册；核心模块可独立编译、独立
测试；Runtime identity、mailbox、binding 与 driver 状态不穿透业务领域；下一次 Runtime 重构主要替换
adapter/composition，而不需要再次修改 Operation、Interaction、Channel 或 WorkspaceModule 核心。
