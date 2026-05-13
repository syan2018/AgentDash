<!-- TRELLIS:START -->
# Trellis Instructions

These instructions are for AI assistants working in this project.

This project is managed by Trellis. The working knowledge you need lives under `.trellis/`:

- `.trellis/workflow.md` — development phases, when to create tasks, skill routing
- `.trellis/spec/` — package- and layer-scoped coding guidelines (read before writing code in a given layer)
- `.trellis/workspace/` — per-developer journals and session traces
- `.trellis/tasks/` — active and archived tasks (PRDs, research, jsonl context)

If a Trellis command is available on your platform (e.g. `/trellis:finish-work`, `/trellis:continue`), prefer it over manual steps. Not every platform exposes every command.

If you're using Codex or another agent-capable tool, additional project-scoped helpers may live in:
- `.agents/skills/` — reusable Trellis skills
- `.codex/agents/` — optional custom subagents

Managed by Trellis. Edits outside this block are preserved; edits inside may be overwritten by a future `trellis update`.

<!-- TRELLIS:END -->

# 用户声明

1. 使用中文和用户交流
2. 这是一个预研期间的项目，当前完全未上线，请规避使用任何兼容性方案、回退方案；也完全不需要考虑API/数据库字段修改相关的问题，让项目保持最正确的状态（但请注意处理数据库migrate）
3. 本项目要求的Commit格式为 `type(scope): 中文提交信息`，并分点描述具体更新内容作为commit备注。
4. 使用 `pnpm dev` 启动调试，其会自动编译 Rust binary，再依次拉起 云端后端 / 本机后端 / 前端；Rust后端无法热重载，更新后重新调试需要杀先前进程。

# 问题收纳

此文件剩余的作用是说明 Agents 在此项目中工作时可能遇到的常见错误和易混淆点。如果您在项目中遇到任何让您感到意外的情况，请提醒与您合作的开发者，并在 AGENTS.md 文件中注明该情况，以帮助防止未来的智能体遇到相同的问题。

## 问题说明

- 通过 PowerShell 把包含中文的 inline Node/Playwright 脚本直接管道给 `node -` 时，中文内容可能在进入浏览器前就被降成 `?`，会让会话输入框和 session 历史里都出现 `????`。如果要做中文端到端浏览器调试，优先使用 UTF-8 文件脚本、Unicode escape，或避免经由当前 PowerShell 管道直接注入中文字符串。
- 如果你是 Codex Agent，在本项目中不要使用 `spawn_agent` 或派发任何 subagent；当前项目环境下 Codex subagent 很容易卡住。需要并行排查时，改为在主会话内用普通命令分步完成，并及时向用户同步进展。
