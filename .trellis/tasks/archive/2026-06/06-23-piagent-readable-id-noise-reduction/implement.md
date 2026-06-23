# PiAgent 可见 ID 统一降噪实施计划

## Pre-Implementation Reads

- `.trellis/spec/backend/session/pi-agent-streaming.md`
- `.trellis/spec/backend/session/context-compaction-projection.md`
- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/cross-layer/backbone-protocol.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/spec/frontend/hook-guidelines.md`
- `.trellis/tasks/archive/2026-06/06-23-piagent-large-output-lifecycle-closeout/{prd,design,implement}.md`

## Checklist

- [ ] 搜索并确认所有 ID 生成/解析点：`stable_tool_result_item_id`、`lifecycle://session/tool-results`、`session/tool-results`、`session/terminal`、`lifecycle_path`、`tool_call_id`、`terminal_id`。
- [ ] 设计并实现 session-scoped alias registry，覆盖 raw turn id -> `turn_001`、raw tool call id -> `tool_001/cmd_001`、raw terminal id -> `term_001`。
- [ ] 将 PiAgent tool result ref context 从 raw `{turn_id}:{tool_call_id}` 改为 readable ref 对象，确保 bounded preview、details、cache write 使用同一对象。
- [ ] 调整 `SessionToolResultCache` metadata：支持 readable aliases、body kind、raw trace 字段和短 lifecycle path。
- [ ] 调整 stream mapper：tool start/update/end、commandExecution、dynamic/native tool item id 使用 `{turn_alias}:{tool_alias/cmd_alias}`。
- [ ] 调整 lifecycle VFS provider：支持 `session/tool-results/{turn_alias}/{body_alias}/metadata.json` 与 `result.txt`，其中 alias 使用 `前缀_ID` 风格。
- [ ] 调整 journey surface：`session/tool-results`、`session/items`、`session/tools`、terminal metadata/log 的文件名和 status 文本使用 readable alias，terminal 使用 `term_001` 风格并纳入本任务验收。
- [ ] 调整 continuation / projected transcript 渲染：主文本保留短 path，不渲染 raw trace。
- [ ] 调整 frontend bounded output parser tests、ToolOutputContentViewer、CommandExecutionCardBody 展示测试，确认短 path 是默认展示。
- [ ] 检查 `SessionMessageCard.tsx` 当前工作区修改是否来自用户；如与本任务一起提交，需确认其 UI 防护属于本任务范围，否则不纳入本任务提交。
- [ ] 更新规格文档，固化 readable alias 与 raw trace 的职责边界。

## Validation

- `cargo test -p agentdash-agent tool_result`
- `cargo test -p agentdash-agent --test runtime_alignment`
- `cargo test -p agentdash-executor pi_agent`
- `cargo test -p agentdash-application lifecycle`
- `cargo test -p agentdash-application projected_transcript`
- `cargo test -p agentdash-application continuation`
- `pnpm --filter app-web test -- boundedOutput useSessionFeed CommandExecutionCardBody`
- `pnpm run contracts:check`
- `pnpm run frontend:check`
- `git diff --check`
- `python ./.trellis/scripts/task.py validate 06-23-piagent-readable-id-noise-reduction`

## Review Gates

- 可见文本 grep：新增或更新测试 fixture 中不应再出现 `lifecycle://session/tool-results/t`、`call_` provider id 或长 hash 作为默认 path。
- Path 一致性：ThreadItem id、bounded preview path、cache key、VFS read path 必须由同一个 readable ref 推导。
- Raw trace 保留：metadata / trace 中必须能找到 raw turn/tool/terminal id，方便调试。
- Projection 安全：continuation、projection、repository rehydrate 不读取 full body，也不把 raw trace 渲染到模型主上下文。
- Frontend 来源：前端不自行生成 alias，只消费后端 path/metadata。

## Risk Points

- Alias registry 如果只放在 AgentLoop 内，lifecycle/journey surface 无法复用同一映射；需要放在 session runtime 级别或把 resolved readable ref 持久进 bounded fact。
- VFS 目前有 provider 与 journey 两条 path parser；只改其中一条会产生可读面不一致。
- terminal full log 当前更多是 status/readback surface，实现 `term_001` 时需要确认 metadata 能把 alias 映回 raw terminal id。
- 工作区已有 `SessionMessageCard.tsx` 非本任务修改，提交前需要避免误收或明确纳入。

## Sub-Agent Dispatch Notes

实现 subagent 必须直接执行任务，不等待其它 subagent 回包。检查 subagent 重点审查：

- 是否仍有 raw provider id / 时间戳 turn id / raw terminal id 出现在默认可见文本。
- readable ref 是否贯穿 producer、Backbone、cache、VFS、journey、continuation、frontend。
- specs 是否只记录新的正确架构和原因。
