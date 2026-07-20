# Cross-Layer Thinking Guide

> 跨层功能实现前的思考清单。大多数 bug 发生在层边界，而非层内部。

---

## AgentDash 关键边界

| 边界 | 常见问题 |
|------|---------|
| Agent ↔ ExecutionManager | Agent 输出格式不统一、执行超时 |
| ExecutionManager ↔ StateManager | 状态回写时机、失败状态处理 |
| Orchestration ↔ State | 编排策略不能直接修改状态（必须通过 StateManager） |
| Injection ↔ Task | 注入内容过大、注入时机 |
| Cloud ↔ Local VFS | mount 语义不一致、绝对路径泄漏、context 与 runtime tool 分叉 |
| Backend ↔ Frontend | 实时状态推送协议、断线重连 |
| Managed Runtime ↔ Driver | canonical Runtime identity、source coordinate 与终态回执 |
| Dashboard DB ↔ Local Runtime DB | migration checksum、顺序升级与最终schema一致性 |

---

## 跨层错误模式

1. **编排层绕过状态层**：编排策略直接修改 Task/Story 字段，绕过 StateManager，导致状态历史缺失
2. **前端自行推断状态**：根据 artifacts 数量等间接信号推断 Task 状态，而非以后端 `status` 字段为准
3. **策略泄漏到接口**：接口暴露了实现细节（如 `createWorktree`），而非表达意图（如 `createWorkspace`）
4. **视图操作影响执行**：删除视图分组时意外修改了 Story 状态——视图关系是展示层概念
5. **产品binding存在 ≠ Driver可用**：必须同时验证Host binding generation/lease与canonical Runtime状态；断连收敛Lost后不能由旧generation复活
6. **同一生命周期实体被跨层重复创建**：Managed Runtime在command admission创建canonical Turn后，Driver的`TurnStarted`只能确认该identity并附带source coordinate；否则一个用户Turn会形成两个Runtime Turn并触发非法状态迁移
7. **业务终态与派发结果混为一谈**：Driver已经发出`TurnTerminal`后，底层任务的同一失败属于已投影的业务结果；dispatch必须完成outbox ack，避免重派一个已经终态的command
8. **只验证主数据库的migration**：Dashboard与本机Runtime各自拥有持久数据库；migration文件一旦被任一实例应用就成为immutable历史，字段演进必须追加新migration并验证所有持久实例顺序升级
9. **展示生命周期存在多个producer**：同一逻辑Item只能有一个presentation owner；执行Broker可保留internal canonical state，但presentation route必须在binding时求值并同时约束Driver mapper与Broker projector
10. **把delivery acceptance当作业务终态**：command被Driver接受、业务Turn结束与terminal event提交是三个独立边界；outbox ack、Operation terminal和重试判定分别依据对应durable事实
11. **过滤后重编号破坏断线cursor**：对外journal cursor必须沿用durable raw sequence；internal-only记录形成的空洞是合法的，live、GET、replay与fork cutoff必须使用同一坐标系
12. **动态surface更新阻塞当前工具回调**：平台AgentFrame/ContextFrame mutation先作为canonical事实接受；需要等待idle的connector同步由outbox延后完成，使当前tool result可以先回灌并结束active turn
13. **schema可见性被误当作工具可执行性**：catalog编译只证明definition存在；production composition还必须用真实AgentFrame、Hook、VFS、permission与workspace owner provider完成调用并继续下一轮provider
14. **continuation handle存在但路由事实未装配**：返回`terminal_id`、cursor或operation handle之前，production composition必须已经注入它后续控制所需的typed owner与registry；局部工具测试不能替代删除装配线即失败的composition测试
15. **producer与admission各自正确但组合非法**：新增Driver/internal/presentation fact或sink admission结果时，必须同时审计producer mapper、Runtime允许集合、全部Native/Codex/Remote pump和outbox settlement；单包测试不能证明跨层闭环
16. **把sink flow-control当作canonical状态**：`Terminalized`等返回值只能控制producer停止；work ack、业务终态与binding收敛必须二次读取durable Runtime事实
17. **把准入版本扩张成生命周期版本**：catalog/surface revision证明新调用在哪个快照上被接纳；canonical Item建立后，稳定owner identity与generation负责后续收敛。工具自身推进surface时旧revision自然过期，因此跨层审计要区分admission fence与accepted entity lifecycle fence
18. **把审计事实与命令式意图混为一谈**：journal可持久化“展示请求曾发生”供审计，但历史
    hydrate不应重放短命UI命令。跨层事件要显式区分projection invalidation与imperative intent，
    再验证live dispatcher、命令式UI owner、workspace-scoped store初始化与最终renderer。
19. **把不同生命周期的 generation 合并为一个坐标**：Product profile、Host binding generation、
    live attachment/connection epoch 与 remote binding generation分别回答“重建什么”“本地第几代
    binding”“当前连接是谁”“远端接受哪一代”。跨层设计要为每个坐标指定唯一owner和显式映射，
    因为任意一个注册期常量都无法代表多个Runtime thread及其重启恢复历史。

---

## 实现检查清单

**实现前：**

- [ ] 映射完整的数据流路径
- [ ] 确认不会绕过 StateManager 进行状态修改
- [ ] 确认视图操作不会影响核心状态
- [ ] 若涉及云端/本机文件访问，先定义 mount/provider/capability 边界（参考 `vfs-access.md`）
- [ ] 若涉及 runtime hook/workflow，确认"信息获取在 loop 外、控制决策在 loop 边界同步"（参考 `execution-hook-runtime.md`）
- [ ] 若command会创建Runtime实体，明确唯一identity owner，并为下游source identity建立独立映射
- [ ] 若事件可由Driver与Broker共同观察，先固定effective presentation route，并在组合测试中断言每个逻辑Item只有一个start/update/terminal序列
- [ ] 若会过滤internal event，固定对外cursor沿用的durable sequence与fork cutoff语义，覆盖含sequence gap的live→replay
- [ ] 若工具能更新AgentFrame/surface，明确canonical mutation、ContextFrame提交与connector idle同步的先后关系
- [ ] 若 durable event 会驱动命令式 UI，画出 persistence → hydration/live dispatcher →
  effect planner → UI owner → scoped store → renderer 全链路，并明确命令在首次 mount effect
  前后都保持相同结果；同时明确durable审计事实是否真的具有历史重放语义
- [ ] 若hydration boundary控制副作用，分别验证“source target是否reset”和“history load是否成功”
  两个状态；在StrictMode的`setup → cleanup → setup`中，首个真正完成的load必须发布boundary
- [ ] 若工具返回可续接handle，确认owner、route registry与retained state owner在返回前同时建立，并覆盖跨owner拒绝及短命令完成后的保留窗口
- [ ] 若新增Driver fact或sink admission枚举，建立“所有producer × Runtime admission × 所有pump/worker consumer”矩阵，并至少增加一条真实persistence组合回归
- [ ] 若工具执行中可能更新catalog/surface，分别列出新调用准入坐标与已accept调用的progress/approval/terminal坐标，并覆盖“发起更新的调用完成 + 更新后的新调用按新revision准入”
- [ ] 若schema被多个进程或数据根消费，列出每个持久实例并验证既有数据库升级，而不只验证空库或Dashboard数据库
- [ ] 若跨Host/transport重写identity或generation，列出Product、Host binding、attachment/connection与remote target四个owner，并为出站改写、callback反向映射、disconnect retire和restart re-materialization各放一条回归

**实现后：**

- [ ] 验证 StateChange 历史完整记录
- [ ] 验证前端状态与后端状态一致
- [ ] 若引入新 runtime policy/metadata，验证前端看到的是真实生效的 runtime surface
- [ ] 验证Driver acknowledgement不会推进第二份canonical lifecycle，并覆盖“终态已发出后底层任务返回失败”的outbox ack语义
- [ ] 对critical violation覆盖`valid prefix -> invalid suffix`：staged prefix零落地、committed revision不漂移、唯一terminal presentation/effect/quarantine同事务提交，pump停止且不补`BindingLost`
- [ ] 用真实持久数据库和production composition覆盖“user → 多工具（含业务错误）→ tool result回灌 → final assistant → disconnect/rebind → 下一轮”，并断言Operation、outbox、cursor和前端card identity
- [ ] 对所有continuation handle运行“start返回 → 原owner续接 → terminal后读取 → 错误owner拒绝”的composition测试，且测试在漏注入registry时必须失败
- [ ] 对外协议升级同时运行generated contract check与前端typecheck，确保generator生成的跨crate类型导入完整
- [ ] 对 durable presentation 使用真实 journal 与 production 页面验证成功事件、面板展开、
  active tab 和 renderer 内容同时成立；planner/executor 单测只证明中间节点，不作为闭环证据
