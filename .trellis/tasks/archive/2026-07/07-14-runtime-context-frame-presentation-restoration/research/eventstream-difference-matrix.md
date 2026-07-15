# ContextFrame eventstream 差异矩阵

参考仓库固定为 `D:/Projects/AgentDash-main-reference@957fa9d60ea3d67efa1bb278fe5b376cf0c34598`。当前实现基线为 `9e9643769`。

## Wrapper 映射

Managed Runtime journal 内部持久化 owned typed event：

```text
Platform.ContextFrameChanged { frame }
```

Session journal contract 在 `agentdash-api::routes::lifecycle_agents::journal_event_to_contract` 的 protocol normalization boundary 单向投影为 main-reference 前端既有形态：

```text
Platform.SessionMetaUpdate { key: "context_frame", value: frame }
```

映射只替换 wrapper discriminant；`value` 直接序列化同一个 owned `ContextFrame`，不重建、不筛字段、不调整 section 顺序。前端不存在 typed/legacy 双解析路径。

## Payload 与顺序矩阵

| 场景 | canonical producer / UoW | main oracle | 允许差异 | payload / section / order 结果 |
| --- | --- | --- | --- | --- |
| bootstrap identity / user / environment / guidelines | exact compiled artifact + `ThreadStart` | main-reference production builders 与 application facade actual-producer journal test | Session wrapper、Runtime coordinate、动态 ID/time | 零内容差异；随 user submission 与 turn start 同交 |
| bootstrap assignment / capability / tool / Skill / Memory / VFS / MCP / companion | Business Agent Surface compiled presentation + `ThreadStart` | main-reference production builders + eight-dimension builder exact assertions | 同上 | 零内容差异；frame 与 section 保持 oracle 顺序 |
| live surface transition | typed previous/target surface delta + `SurfaceAdopt` | `wi03_surface_adopt_stream_main_957fa9d.json` | wrapper、coordinate | 零内容差异；`HookPlanBound` 后追加 presentation；empty delta 不产出；replay 不重复 |
| ToolSchema 参数摘要 | adoption ToolSchemaDimensionDelta projector | `wi03_tool_schema_formatter_main_957fa9d.json` | 无 | nested object/array、anyOf/enum、required、稳定顺序、描述截断、48-field truncation 全等价 |
| Hook model-visible effect | `HookRun` terminal/effect UoW | main-reference Hook/launch production builders + typed Hook actual-producer test | wrapper、coordinate、动态 ID/time | 零内容差异；silent observer 不产出；replay 不重复 |
| pending action | next `TurnStart` | `wi03_pending_action_stream_main_957fa9d.json` | wrapper、coordinate、动态 ID/time | 零内容差异；正文、instruction、injection usage 与 main 一致 |
| Hook auto-resume / system delivery | next `TurnStart` | main-reference `build_system_delivery_context_frame` + actual `SystemDelivery` command test | wrapper、coordinate、动态 ID/time | `system_delivery` 内容零差异；不使用保留的 `auto_resume` family |
| managed compaction | checkpoint/head activation UoW | `wi03_compaction_stream_main_957fa9d.json` | wrapper、coordinate、动态 ID/time | 零内容差异；opaque driver compaction 不产出 |

## 前端行为矩阵

对 `packages/app-web/src/features/session` 与 main-reference 同目录执行逐文件 diff：只有 6 个非 ContextFrame 的既有小差异（`companionSubagentDispatch.ts`、`sessionStreamReducer.ts` 的 command null typing、`SessionEntry.tsx`、`ToolCallCardShell.tsx`、`CompanionSubagentDispatchCardBody.tsx`、`ReadCardBody.tsx`）。以下 ContextFrame 路径均无源代码差异：

| 行为 | 文件 | 结果 |
| --- | --- | --- |
| payload parsing / null semantics / section order | `model/contextFrame.ts` | 与 main 零差异 |
| event → display entry | `model/sessionStreamReducer.ts` 的 ContextFrame 分支 | 与 main 零差异 |
| consecutive frame aggregation | `model/useSessionFeed.ts` | 与 main 零差异 |
| ContextFrame 截断 tool burst hard-boundary | `model/useSessionFeed.ts` | 与 main 零差异 |
| single/multi frame card、tab、body、section 文案 | `ui/ContextFrameCard.tsx`、`ContextFrameStream.tsx`、`ContextFrameBody.tsx`、`ui/contextFrame/SectionRenderers.tsx` | 与 main 零差异 |
| SessionEntry 聚合入口 | `ui/SessionEntry.tsx` 的 ContextFrame 分支 | 与 main 零差异 |

## 依赖方向审计

| 层 | ContextFrame 职责 | 审计结果 |
| --- | --- | --- |
| Application source | AgentFrame / workflow phase / tool / Hook 等 typed facts | 不构造 Driver DTO；production phase 来自 runtime surface provenance |
| Business Agent Surface / Runtime | 编译 immutable presentation plan、typed delta 与 canonical producer | owned 唯一 builder；与 canonical mutation 同 UoW |
| API | composition；Session journal boundary 做 typed wrapper 等价映射 | 不重建 payload、不拥有 frame family 业务规则 |
| Native / Codex / Remote adapter | 只翻译 materialized driver surface 与 vendor lifecycle | 无 ContextFrame builder、projector 或 AgentFrame repository 读取 |
| Frontend session | 消费 main 等价 `session_meta_update/context_frame` | reducer/feed/UI 无新分支、无第二套 renderer |

## 验证记录

2026-07-14 使用单实例 `pnpm dev` 完成真实组合启动：server health、Local Runtime claim、backend registration 与 Vite 均成功。Personal auth 为无需登录模式，历史 AgentRun journal 可通过真实 HTTP API 读取，返回的仍是既有 Session `notification.event` contract。

默认 `general` Project Agent 的 execution config 只有 `executor = PI_AGENT`，第一次尝试因此在 runtime binding 前被拒绝。继续通过只读 API 查询发现本地 `openai-codex` provider 为 enabled/executable，global credential 来自数据库，且 `gpt-5.4` 是可执行模型；随后通过正常 Project Agent API 创建临时 `PI_AGENT/openai-codex/gpt-5.4` 验证 Agent，并成功 launch 真实 AgentRun。

真实 prompt 要求调用 `workspace_module_list`。journal 共 12 条：user input 与 turn start 后，seq 3 返回 `platform` wrapper，其中 `payload.kind = session_meta_update`、`data.key = context_frame`，frame kind 为 `capability_state_delta`，section 为 `tool_schema_delta`；没有任何 `context_frame_changed` 泄露到 Session contract。seq 4 起进入 dynamic tool call，随后完整出现 started/completed、tool result、assistant message 与 turn terminal，ContextFrame 位于 tool burst 之前并形成 hard boundary。Runtime context projection 最终 `head_event_seq = 12`、`message_count = 6`，包含 tool call/result；turn 正常结束且开发日志没有新增 runtime dispatch/loop error。

该次真实工具业务执行返回 `runtime surface query missing anchor: component=workspace_module_visibility`，错误被正常物化为 tool result，未造成 turn 卡死。这表明消息、ContextFrame、工具 lifecycle 与 terminal 链路已贯通，同时暴露出独立的 workspace module surface anchor 配置/绑定问题；后者不改变本工作项的 eventstream 边界结论。

可重复的实现验证覆盖如下：

- API exact fixture test 对 main oracle 中每一种 ContextFrame 逐条断言：仅 wrapper 变为 `session_meta_update/context_frame`，payload 完整不变。
- Agent Runtime、Application AgentRun、API journal projection 测试全部通过，覆盖 bootstrap、SurfaceAdopt、Hook、pending action、auto-resume 与 compaction producer。
- 前端 ContextFrame 定向测试 95 项、全量前端测试 585 项及 contracts check 通过。
- 全量前端 `check` 的 typecheck 通过，lint 被工作区既存的 33 个 React Compiler 诊断阻断；本工作项没有修改这些文件。
- in-app browser 的 Node REPL 在导入 browser client 时发生 `Cannot redefine property: process` 初始化冲突；遵循浏览器验证约束未改用旁路自动化，因此本轮没有把视觉检查声明为已完成。
