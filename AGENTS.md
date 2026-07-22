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
3. 本项目要求的Commit格式为 `type(scope): 可保留英文专业用词的中文提交信息`，并分点描述具体更新内容作为commit备注。
4. 使用 `pnpm dev` 启动调试，其会自动编译 Rust binary，再依次拉起 云端后端 / 本机后端 / 前端；Rust后端无法热重载，更新后重新调试需要杀先前进程。
5. 更新文档时不要瞎记录“不要做什么”，只应该记录“为什么这么做”。项目朝着整洁的方向迈进，没有人会关心过去的错误实现是什么样的。更不要记那些只对当前任务有意义而对模块开发没有任何价值的废话。

# 问题收纳

此文件剩余的作用是说明 Agents 在此项目中工作时可能遇到的常见错误和易混淆点。如果您在项目中遇到任何让您感到意外的情况，请提醒与您合作的开发者，并在 AGENTS.md 文件中注明该情况，以帮助防止未来的智能体遇到相同的问题。

## 问题说明

- 通过 PowerShell 把包含中文的 inline Node/Playwright 脚本直接管道给 `node -` 时，中文内容可能在进入浏览器前就被降成 `?`，会让会话输入框和 session 历史里都出现 `????`。如果要做中文端到端浏览器调试，优先使用 UTF-8 文件脚本、Unicode escape，或避免经由当前 PowerShell 管道直接注入中文字符串。
- 小规模迭代时不要过度、为了不会有影响的修改重复测试，这只会浪费时间，不会带来任何真实的安全性
- 任何时候禁止为了完成自己的任务碰工作区存在的修改，即使这些修改导致测试失败。不要摧毁并行会话的工作成果
- VS Code / rust-analyzer 可能自动运行 `cargo check --workspace --all-targets` 并长时间占用 Cargo build directory 锁；手动 Cargo 命令看似卡住时，先观察当前 `cargo` / `rustc` / `rust-analyzer` 进程，因为等待锁通常比强行终止更能保留 IDE 与并行会话的缓存状态
- Windows 上 `pnpm dev:desktop` 可能出现 WebView2 `0x80070057` 创建失败但 `agentdash-local-tauri` 进程仍存活；此时 renderer 和登录桥接并未运行，不能以壳进程存在判断 Desktop Runtime credential claim 已触发，应结合窗口内容、Tauri日志与server端`last_claimed_at`确认真实链路
- `postgresql_embedded` 测试若并发复用同一 data root，多个 PostgreSQL 启动流程可能在初始化阶段相互竞争并失败，且不会进入业务断言；需要让共享 data root 的 embedded PostgreSQL 测试串行启动，或为测试分配隔离的数据目录。
- `cargo fmt --all` 会解析 workspace 内所有 crate；若 `agentdash-agent-runtime-test-support` 的 `#[path]` 指向本机不存在的 `AgentDash-main-reference` checkout，格式化会在读取模块前失败。此时应先确认 reference checkout 配置，任务内文件可使用相同 toolchain 的 `rustfmt --edition 2024 <files>` 做定向格式化。
- Windows 上 `cargo fmt --all` 可能因源码文件被用户映射区域短暂占用而报 `os error 1224`；未格式化文件可使用相同 toolchain 的 `rustfmt --edition 2024 <files>` 定向完成，随后再用 diff 与编译检查确认结果。
