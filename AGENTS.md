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
- `PI_AGENT` 在会话页里执行 shell 任务时，模型有时会把工作空间绝对路径（如 `F:\Projects\AgentDash`）直接塞进 `shell.cwd`。当前 Hook Runtime 已会把“位于 workspace root 内的绝对 cwd”自动 rewrite 成相对路径，因此这类 shell 调用通常不会再因绝对路径直接失败；如果仍看到 cwd 相关错误，优先排查是否为非 shell 工具、路径已越出 workspace root，或查看历史 session jsonl 是否来自修复前的旧会话。
- `crates/agentdash-injection/src/address_space.rs` 里已经有一个名为 `AddressSpaceProvider` 的 trait，但它当前只负责 address space descriptor / 能力发现，不是“统一 read/write/list/search/exec 访问层”里的目标 provider。推进统一 Address Space 方案时，不要因为同名就误判为底层访问抽象已经存在；需要明确是扩展、替换还是重命名这一层。
- `.trellis/scripts/task.py create` 会自动给任务目录补 `MM-DD-` 日期前缀；如果传入的 `--slug` 自己已经带日期前缀，就会生成类似 `03-22-03-22-...` 的双日期目录。创建 task 时，`--slug` 应只写语义名，不要重复带日期。
- `pnpm dev` 走的是 [scripts/dev-joint.js](F:/Projects/AgentDash/scripts/dev-joint.js)，它在启动时会先编译一次 Rust binary，再拉起 `agentdash-server` / `agentdash-local` / 前端；如果你修改了 Rust 代码但没有完整重启 `pnpm dev`，浏览器里很可能仍然跑的是旧后端。另一个高频坑是残留的 `agentdash-local` 会因为重复 `backend_id` 注册把整套 dev 服务带崩；当前脚本已默认在启动前按进程名清理 `agentdash-local`，如果仍遇到“本机后端重复注册被拒绝”，优先检查是否用了 `--skip-local`、是否还有手工启动的同名本机后端，或是否是历史旧脚本未生效。
