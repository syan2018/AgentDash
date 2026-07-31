# main-reference ContextFrame 对照

## 对照范围

- 工作树：`D:\ABCTools_Dev\AgentDash-main-reference`
- revision：`957fa9d60ea3d67efa1bb278fe5b376cf0c34598`
- 重点文件：
  - `crates/agentdash-application-runtime-session/src/session/*_context_frame.rs`
  - `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs`
  - `crates/agentdash-spi/src/hooks/mod.rs`
  - `packages/app-web/src/features/session/ui/ContextFrameStream.tsx`

## 可以继承的设计

main-reference 的 Frame 粒度是“一个 Agent 可理解的语义单元”，不是“一个上游
contribution”：

- Identity Frame 同时聚合 base system prompt、Project Agent identity 与 agent prompt，
  来源作为有序 fragments 保留。
- SystemGuidelines Frame 同时聚合 user preferences 与已发现的项目 guidelines，
  内容作为多个 typed sections 保留。
- Assignment Frame 收集同一 assignment scope 下的有序 fragments。
- CapabilityStateDelta Frame 一次承载 capability、tool schema、skill、MCP、VFS、memory、
  companion 等多个变更维度。
- `rendered_text`从同一份 sections/fragments生成，而不是和结构化内容分别维护。

因此它避免了“两个 identity contribution 就显示两个 Identity Frame”这种来源泄漏。

## 不应恢复的设计

main-reference 后期在 `5fc544fee` 引入了 `ContextDeliveryPlan`、delivery metadata、
cache policy、model channel、connector profile 与 consumption mode，用来驱动
RuntimeSession `TurnPreparer`和具体 Connector。

`af21f9d7c` 已删除整套 RuntimeSession、Connector 与对应 delivery planner。当前代码中：

- `ContextDeliveryPlan`、`ContextDeliveryTarget`没有生产消费者；
- `ContextDeliveryEntry`只由`ContextFrame::delivery_entry()`构造，也没有消费者；
- cache policy没有cache行为；
- model channel不控制provider输入，Dash provider直接拼接全部Frame文本；
- connector profile没有connector协商；
- `phase_node`在生产Frame中始终为空；
- `apply_mode`、message role、delivery channel主要只用于前端调试展示；
- top-level source在生产路径中固定为`RuntimeContextUpdate`，真实来源已经存在于section；
- `SystemAppend`被借作active fold分类标记，是旧connector概念承担了新的history语义。

恢复这套planner会重新引入已被架构切换删除的owner和状态机，不符合当前Complete Agent边界。

## 当前实现为何更碎

`accepted_context.rs::materialize_surface_frames()`逐个遍历
`DashSurface.instructions`，每条非capability instruction生成一个Frame。
Product provisioning又把`AgentContextSourceSnapshot.fragments`逐条映射为Surface
instruction，因此Frame数量与upstream contribution数量直接相等。

此外当前只有Identity presentation使用Identity section；Environment、
SystemGuidelines、MemoryContext、UserContext与AssignmentContext全部被装进
`AssignmentContext` section。协议虽保留了Environment、UserPreferences、
ProjectGuidelines、UserContext等旧section，但当前生产路径从不构造它们。

现有测试已经把这一错误形态锁成预期：

```text
Identity
SystemGuidelines
Identity
Environment
AssignmentContext
CapabilityStateDelta
MemoryContext
UserContext
```

这里两个Identity来自两个contribution，不是两个Agent语义阶段。

## 收敛原则

1. ContextFrame粒度由Agent消费语义决定，来源粒度只进入Frame内部。
2. stable语义最多各有一个active Frame：
   Identity、UserContext、Environment、SystemGuidelines、AssignmentContext、MemoryContext。
3. 同类来源按原始order聚合为一个通用fragment section；不伪装成其他typed section。
4. CapabilityStateDelta仍按一次Surface transition聚合全部变更维度。
5. CompactionSummary等occurrence Frame按发生事实保留，不按kind合并。
6. Initial Context与Surface共同参与stable Frame编译，避免两个owner分别产出同kind Frame。
7. 编译器属于Dash Complete Agent内部；Native adapter只转换Bound Surface事实。
8. History event保存本次Agent实际接受的Frames；Frame不再嵌套在Surface/Installation来源对象里。
9. history event决定replace、append、reset语义，不再让connector consumption metadata承担fold分类。
10. 只保留真实影响Agent上下文的Frame；audit-only/ignore presentation不伪装成ContextFrame。

## 推荐的深模块接口

不新增公开planner、batch、snapshot或generation。Dash内部保留三个use-case入口：

```rust
compile_initial_context(installation)
compile_surface_update(installation, current_surface, previous_surface)
compile_surface_revoke(installation, previous_surface)
compile_compaction_rebuild(installation, current_surface, summary)
```

三个入口共享同一套内部步骤：

```text
collect accepted system sources
-> group stable sources by ContextFrameKind
-> preserve ordered fragments
-> derive typed transition sections
-> render exact Agent-visible text
-> stamp occurrence ID/status
-> return canonical ordered Vec<ContextFrame>
```

这是一个深模块：调用方只表达发生了哪种accepted fact，不参与分组、渲染、排序或metadata拼装。
