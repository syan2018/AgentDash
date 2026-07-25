# Product 控制面与 Runtime Hard Cut 边界审计

## 结论

本任务只收敛 Agent Runtime 内核及其稳定接缝。Application/Product 负责把既有业务
适配到 final Runtime Contract、typed Tool Broker、typed Hook owner、
AppliedResourceSurface 与 canonical conversation；Product 领域本体不进入 Hard Cut。

模块缺少编译错误或页面仍能展示历史内容，不能证明 Product 能力仍然存在。可靠证据必须
同时覆盖生产 route、AppState composition、真实写入 caller、持久化 owner 与纵向
behavior tracer。

## 为什么控制面缺席没有立即表现为构建失败

分支历史显示，Product 控制面曾按以下顺序退出真实构建图：

1. `c3cc58b9` 收窄 AgentRun exports 与 Infrastructure Runtime composition；
2. `8d22d6cf` 从 Rust module graph 卸载 Canvas、Capability、Companion、
   Frame Construction、Routine、Runtime Tools、Wait、Lifecycle orchestration/VFS
   与 Hook；
3. `a535ae01` 同步移除 AppState services、HTTP routes 与 Project AgentRun create
   入口；
4. `088fa55b` 再物理删除已经不可达的 Product 源码；
5. 后续 `7b88661d`、`2e653ab1`、`ad05facf` 分别恢复 Frame/Hook、Product modules 与
   production composition。

Rust 只编译 module graph 可达源码；模块卸载时其中的 `#[cfg(test)]` 也一起失去覆盖。
route 与 caller 同时缺席只会让能力变为不可调用，并不会产生类型错误。前端使用 mock API
或历史 canonical fixture 的测试仍可通过；只读 projection 仍能展示旧数据，但不能证明
新的 Product command 可以产生同类事实。

因此，构建通过与旧记录可见只能作为读路径证据，不能作为 replacement 或 deletion
evidence。

## 当前稳定 owner

| Product 能力 | 必须保留的 owner | Runtime 适配点 |
| --- | --- | --- |
| AgentFrame / Surface | Domain repository、Frame Construction、AgentRun frame/surface policy | ProductLaunch、AppliedResourceSurface、Activate |
| Companion | relation、channel、gate、continuation、mailbox、adoption、result repositories/workers | typed Product Tool command、Fork/Create/Activate/Input |
| Workspace Module | descriptor、operation、visibility、presentation store/feed、API | typed Product Tool command、AgentRun surface |
| Routine / Workflow | trigger、reuse、gate、dispatch、receipt、terminal policy | ProductLaunch、ProductInputDelivery、Runtime changes |
| Hook | Product presets、rules、scripts、effects | typed Product event owner / Complete Agent callback |
| Canvas / Terminal / Wait / VFS | 各自 Product control、repository、projection 与 API | typed Tool command、AppliedResourceSurface、Product feed |

可以进入 Hard Cut 的是已经被 final seam 等价替代的旧
`runtime_facade/runtime_session_boundary/journal`、universal
`RuntimeToolProvider`/composer、aggregate Hook execution adapter 和零消费者 SPI；
Product command、route、repository、worker、effect 与用户可见语义均不属于这些壳。

## 当前必须闭合的能力缺口

### Workspace Module 工具集合

`frame_construction/plan.rs` 与 `capability/tool_catalog.rs` 仍声明：

- `workspace_module_list`
- `workspace_module_describe`
- `workspace_module_operate`
- `workspace_module_invoke`
- `workspace_module_present`

当前 production Broker 只有 `workspace_module_present`。历史实现中的 list/describe
读取 Extension 与 Canvas descriptor；operate 执行 Canvas create/attach/copy 并更新
Agent surface；invoke 调用 Extension action/channel/backend service 或 Canvas；
present 负责 Product presentation 并可能提交 Canvas surface update。它们是 Product
业务语义，必须通过干净的 typed Product command seam 等价恢复，然后才能删除旧
Runtime bridge/context/provider。

### Product AgentRun 生命周期

当前 Project AgentRun create route 与 AgentRun Runtime Resume/Close/Delete 的完整
production tracer 尚未闭合。ProjectAgent 模板删除和运行中的 AgentRun Runtime Close
是不同 Product 语义，分别由各自 owner 处理。

### Final Broker producer

Companion durable saga/worker 已进入 production composition，但
`companion_request/respond` 必须由 final Broker 的真实 Host callback 产生；直接构造
Product tool 的单元测试不能替代这条证据。

## C3/C4 最小纵向门禁

1. 使用真实 `create_router(AppState)` 与 production composition 创建 Project
   AgentRun，验证 Product graph、Frame、Runtime binding、Activate、首输入及 canonical
   output，并覆盖 Resume/Close/Delete。
2. 从 Complete Agent Host callback 调用 Companion 工具，经 final Broker 到 Product
   saga、child Frame/Runtime、channel/gate/mailbox/result，并验证重启后沿同一 identity
   续跑。
3. 建立 Surface/Catalog 闭包不变量：每个 Applied AgentFrame 声明的静态 tool 都能由
   production Broker 解析；逐项执行 Workspace Module 五个工具并验证 Product
   repository、surface update 与 presentation feed。
4. 用真实 Router contract tracer 固定 AgentRun create/workspace、Companion gate、
   Workspace Module、Canvas、Terminal 与 VFS surface routes；前端垂直测试消费真实
   snapshot/change/feed，而不是只断言 mock URL。

