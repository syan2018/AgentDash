# 会话标题自动生成

## Goal

用户发出首轮 prompt 后，系统异步调用 LLM 自动生成简短中文会话标题，覆盖到 `session.title`。用户可随时手动修改标题，修改后不再被自动覆盖。行为与 ChatGPT 自动生成对话标题一致。

## Requirements

### 数据模型
- sessions 表新增 `title_source TEXT NOT NULL DEFAULT 'user'`（值域：`user` / `auto`）
- `SessionMeta` 结构体新增 `title_source: TitleSource` 枚举字段
- `TitleSource::User` 表示手动设定（锁定不覆盖），`TitleSource::Auto` 表示 LLM 自动生成

### 后端逻辑
- 触发点：`SessionHub.start_prompt()` 中检测首轮（`last_event_seq == 0`）且 `title_source != User`
- 异步 spawn title 生成任务，不阻塞主 prompt 流程
- 复用会话自身的 executor LLM（从 `executor_config` 解析 provider/model，通过现有 LlmBridge 调用）
- System prompt：要求生成 10 字以内的简洁中文标题，只输出标题本身
- 生成成功后更新 `session.title` 和 `title_source = Auto`，通过 SSE 广播 `SessionMetaUpdated` 通知前端
- 失败时保留原标题，打 warn 日志，不影响主流程
- 防重复触发：用标记位防止并发/快速连续 prompt 重复生成

### API
- `GET /sessions` 和 `GET /sessions/{id}/meta` 返回体增加 `title_source` 字段
- 新增 `PATCH /sessions/{id}/meta` 端点，接收 `{ title }` 并设 `title_source = user`
- SSE 通知新增 `SessionMetaUpdated` 事件类型

### 前端
- Session 页面标题区域支持 click-to-edit（或铅笔图标触发编辑）
- 编辑确认后调用 `PATCH /sessions/{id}/meta`
- 通过 SSE `SessionMetaUpdated` 事件驱动 store 刷新标题显示
- Session 列表/侧边栏无需改变渲染逻辑，title 更新后自动反映

## Acceptance Criteria

- [ ] 新建独立会话发送首条 prompt 后，标题在 1-3 秒内自动更新为 LLM 生成的简短中文标题
- [ ] Task 绑定的会话（title 来自 Task）不触发自动生成（title_source 初始为 user）
- [ ] 无 executor_config 的会话跳过自动生成
- [ ] LLM 调用失败时保留原标题，不报错给用户
- [ ] 用户可在 Session 页面点击标题进入编辑模式，修改后保存
- [ ] 用户手动修改标题后，后续 prompt 不再触发自动生成
- [ ] 前端通过 SSE 实时接收标题更新，无需刷新页面

## Technical Notes

### 复用现有设施
- LLM 调用：复用 `LlmBridge` 抽象 + `PiAgentProviderRegistry`，参考 `compaction/mod.rs` 的模式
- SSE 通知：扩展现有 notification 通道，新增 `SessionMetaUpdated` 事件类型
- 可提取共享的 `build_bridge_from_config()` 工具函数供 title 生成和 compaction 共用

### 涉及文件
| 层 | 文件 | 变更 |
|----|------|------|
| DB | `migrations/NNNN_session_title_source.sql` | 新增 migration |
| Application | `session/types.rs` | 增加 `TitleSource` 枚举和字段 |
| Application | `session/hub.rs` | 首轮检测 + spawn 异步生成 |
| Application | `session/persistence.rs` | 读写 `title_source` |
| Application | 新增 `session/title_generator.rs` | 封装 LLM title 生成逻辑 |
| Infrastructure | `persistence/session_persistence.rs` | SQL 读写 `title_source` 列 |
| API | `routes/session.rs` | 新增 PATCH meta 端点 |
| 前端 Service | `services/session.ts` | 新增 `updateSessionMeta()` |
| 前端 Store | `sessionHistoryStore.ts` | 处理 meta 更新事件 |
| 前端 UI | Session 页面标题区域 | 可编辑标题组件 |
| 前端 Types | `types/session.ts`, `types/acp.ts` | 增加 `titleSource` 字段 |

### 边界情况
- 会话无 executor_config → 跳过
- LLM 返回空/异常 → 保留原标题
- 快速连续 prompt → 防重复标记
- Task 绑定会话 → title_source 初始为 user，不覆盖
