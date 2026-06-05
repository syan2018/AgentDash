# [Parent] Session 输入链路统一与多模态 composer

> 注：本任务 dir slug 为历史名（`session-composer-redesign-image-input`），范围扩大后升级为 **parent**，统筹下列 4 个 child。

## 程序目标

把 Session 页"用户输入 → 模型"这条链路从**视觉**到**协议**整体收敛：
1. 协议层：消灭 ContentBlock / codex UserInput / 多个有损 text flattener 并存的乱象，统一到一条 canonical 路径，**让多模态（图片）结构化直达 `ContentPart::Image`**。
2. 产品层：composer 较大重构 + 内联模型/推理选择器 + 图片输入（粘贴/拖拽）+ 双模追加消息（排队/Steer）。

## 关键事实（研究产出，详见 research/）

- **图片当前是装饰性的**：pi_agent 的 `prompt`/`steer`/`continuation` 三路都把输入拍平成 `"[引用图片: ...]"` 文本，模型看不到真图。模型层 `ContentPart::Image{mime_type,data}` 已存在但无人构造。→ 协议统一是图片功能的**硬前置**。详见 [research/userinput-canonical-path-audit.md](research/userinput-canonical-path-audit.md)。
- **send/steer 入口形态分裂**：message 收 ACP ContentBlock(`prompt_blocks`)，steer 收 codex UserInput；两者最终都收敛到连接器。ContentBlock 在后端仍 load-bearing（`PromptPayload::Blocks` + 4 处调用），清理是真实重构。
- **双模追加消息缺口**：action 模型用 `delivery_running` 把 send_next 与 steer 互斥，running 态唯一出口是 steer，无"排队为下一轮"语义/UI/持久化。后端有 `push_session_notification`(合并进当前轮) 原语但无 HTTP route。详见 [research/send-steer-protocol-and-dual-mode-gap.md](research/send-steer-protocol-and-dual-mode-gap.md)。

## 核心决策

- **canonical user-input 类型**：当前直接采用 codex `UserInput`，但在本项目 protocol crate 内以**别名/newtype 封一层**（调用方依赖项目自有名，不直接 `use codex::UserInput`），为后续转自定义扩展类型预留接缝（与 agent-protocol 现有"先用 codex 类型、后扩展"的惯例一致）。
- **ContentBlock 收敛**：仅保留在 ACP relay 边界（远程后端互通），转换集中一处；删除散落的重复 flattener。
- 图片走 base64/`ContentPart::Image` 结构化，不内联拍平；不做对象存储（本程序范围外）。

## 子任务地图（按依赖依次执行）

1. **child-1 [06-04-userinput-canonical-path]** — 协议一致路径（硬前置，先做）。
2. **child-2 [06-04-image-input-e2e]** — 图片端到端（依赖 child-1，否则模型看不到图）。
3. **child-3 [06-04-composer-redesign-model-selector]** — composer 重构 + 内联模型/推理选择器（前端，可与 child-2 并行但建议 child-1 后；模型选择器与 child-4 的 steer 只读态耦合）。
4. **child-4 [06-04-dual-mode-append-messaging]** — 双模追加消息（排队/Steer）。**排队走服务端托管完整状态**（后端 pending 队列领域状态 + 自动派发 + 事件投影）+ 前端投影 UI。依赖 child-1；与 child-3 协同。规模较大，design 阶段可再自拆子任务。

依赖关系写在各 child 工件内，不靠树位置隐含。父任务保留 planning，待全部 child archive 后做集成 review（遵循"父任务不要早归档"）。

## 跨 child 验收

- [ ] 端到端：从 composer 粘贴/拖入一张图 + 文本发送，**模型实际收到图片**（`ContentPart::Image`），非占位文本；steer 注图同样生效（执行器支持时）。
- [ ] 协议层只剩一条 canonical user-input 表示贯穿 API→应用→连接器→AgentMessage；冗余 flattener 删除或集中；TS 契约重新生成且前端编译通过。
- [ ] composer 为较大改版的一体化布局，含内联模型/推理选择器；保留 @ 引用、token、helperText、promptTemplates。
- [ ] running 态可在"排队（轮次结束自动接续）"与"Steer（立即注入）"间选择；capability 不支持 steer 时自动退化为仅排队。
- [ ] 各层 `lint`/`typecheck`/`test`（前端 app-web + 后端相关 crate）通过；新增/改动逻辑有测试覆盖。

## 范围外

- 图片对象存储/服务端持久化路径；非图片附件（PDF/音频）；排队消息的跨端持久化可视化（child-4 内列为可选增强）。
