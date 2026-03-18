<!-- TRELLIS:START -->
# Trellis Instructions

These instructions are for AI assistants working in this project.

Use the `/trellis:start` command when starting a new session to:
- Initialize your developer identity
- Understand current project context
- Read relevant guidelines

Use `@/.trellis/` to learn:
- Development workflow (`workflow.md`)
- Project structure guidelines (`spec/`)
- Developer workspace (`workspace/`)

Keep this managed block so 'trellis update' can refresh the instructions.

<!-- TRELLIS:END -->

# 用户声明

0. 使用中文和用户交流
1. 这是一个预研期间的项目，当前完全未上线，不需要准备任何兼容性方案；也完全不需要考虑API/数据库字段修改相关的问题，让项目保持最正确的状态

# 问题收纳

此文件剩余的作用是说明 Agents 在此项目中工作时可能遇到的常见错误和易混淆点。如果您在项目中遇到任何让您感到意外的情况，请提醒与您合作的开发者，并在 AGENTS.md 文件中注明该情况，以帮助防止未来的智能体遇到相同的问题。


## 问题说明

- 通过 PowerShell 把包含中文的 inline Node/Playwright 脚本直接管道给 `node -` 时，中文内容可能在进入浏览器前就被降成 `?`，会让会话输入框和 session 历史里都出现 `????`。如果要做中文端到端浏览器调试，优先使用 UTF-8 文件脚本、Unicode escape，或避免经由当前 PowerShell 管道直接注入中文字符串。
- `PI_AGENT` 在会话页里执行 shell 任务时，模型有时会先把工作空间绝对路径（如 `F:\Projects\AgentDash`）直接塞进 `shell.cwd`，导致首个 tool call 因“路径必须是相对于工作空间根目录的相对路径”失败；随后它通常会自行改成 `.` 或相对路径并重试成功。排查这类问题时，不要只看首个失败 tool call，要同时检查后续是否出现成功重试，以及服务端持久化的 session jsonl 历史。
