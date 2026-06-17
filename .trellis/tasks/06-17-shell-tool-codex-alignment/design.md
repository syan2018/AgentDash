# Shell 工具扩展与 Codex 行为对齐 Design

## Architecture

目标形态是将本机 shell 执行抽象为 `ShellSession`，让非交互式 shell 工具和交互式 terminal 共享生命周期、输出缓存和状态模型。

建议边界：

- `agentdash-relay` 定义跨云端/本机协议：shell start/read/write/terminate 命令、响应和事件。
- `agentdash-local` 持有本机 `ShellSessionManager`：spawn process、保留输出、stdin、terminate、retention、prune、relay event emit。
- `agentdash-api` 只维护 relay pending 的单次 RPC；后台 shell 生命周期由本机 runtime 持有，云端只缓存 session metadata 和状态 projection。
- `agentdash-application` 的 VFS exec adapter 将工具层 `ExecRequest` 映射到 shell start/wait 语义，返回模型可消费的初始结果。
- `packages/app-web` 使用同一个 terminal store 展示 shell session 和手动 terminal session，command card 与 terminal tab 指向同一个真实 `terminal_id` / `process_id`。

## Codex Alignment

Codex 的关键行为需要对齐为 AgentDash 合同：

- `yield_time_ms` 是单次工具调用等待窗口，不是进程生命周期上限。
- 初始调用在 `yield_time_ms` 内收集输出；进程仍运行时返回 `process_id`、partial output、`next_seq` 和 running 状态。
- 空 `write_stdin` 等价于 wait/poll；非空 input 是真实 stdin 写入。
- 输出通过实时 delta、read snapshot、end/closed state 三条链路传递。
- 输出缓存有上限，稳定保留头尾，并报告 omitted/truncated 信息。
- 进程表有上限，退出进程短期保留，超限时优先清理已退出旧进程。

## Protocol Shape

建议新增协议，不在旧 `response.tool.shell_exec` 上挤太多语义：

- `command.tool.shell_start`
  - input: `call_id`、`command`、`mount_root_ref`、`cwd`、`timeout_ms`、`yield_time_ms`、`max_output_bytes/tokens`、`tty`
  - output: `call_id`、`session_id`、`process_id`、`state`、`exit_code`、`stdout/stderr/pty` partial、`next_seq`、`truncated`
- `command.tool.shell_read`
  - input: `session_id/process_id`、`after_seq`、`wait_ms`、`max_bytes`
  - output: ordered chunks、`next_seq`、`state`、`exit_code`、`closed`
- `command.tool.shell_input`
  - input: `session_id/process_id`、`data`
  - output: accepted / stdin_closed / unknown_process
- `command.tool.shell_terminate`
  - input: `session_id/process_id`
  - output: running/killed/already_exited
- events:
  - `event.tool.shell_output_delta`
  - `event.tool.shell_state_changed`
  - optional bridge event to terminal platform output/state for frontend reuse

旧 `command.tool.shell_exec` 可以在同一版本内改为调用 start + wait，并返回新结构；因为项目处于预研未上线阶段，不需要保持旧响应兼容。

## Local Runtime

`ShellSessionManager` 建议维护：

- `HashMap<ShellSessionId, ShellSession>`
- monotonic `seq` 输出序号
- retained output buffer，按 stream 分类并保留 head/tail
- process state: starting/running/exited/failed/killed/lost/closed
- stdin writer
- output notify，用于 read wait
- exit watcher 和 trailing output grace
- max sessions / LRU prune / exited retention

对于执行模式：

- 非交互默认使用 pipe stdout/stderr，适配模型工具。
- `tty=true` 使用 PTY，适配需要交互 TTY 的命令和 terminal tab。
- 前端 terminal tab 统一消费同一个 session output buffer；PTY 输出 stream 标记为 `pty`。

## Codex Reuse Assessment

可直接评估引用的轻量公开 crate：

- `codex-utils-pty`：公开 pipe/PTY spawn、`ProcessHandle`、`SpawnedProcess`、`TerminalSize`，可替换当前零散的 `tokio::process` 与 `portable_pty` 直接使用。
- `codex-utils-output-truncation`：公开文本截断和 token 估算工具，可用于最终模型输出截断。

谨慎评估、不默认引入的较重 crate：

- `codex-exec-server`：公开 `ExecParams`、`ReadParams`、`WriteParams`、`TerminateParams`、`ExecOutputStream`、`ProcessOutputChunk` 等协议类型，行为最接近我们需要的 read/write/terminate 模型；但它会带入远程环境、文件系统、沙盒和 HTTP client/server 等较大依赖面。本项目不需要 Codex 的完整 exec-server runtime，因此第一阶段只参考其协议与 read 模型，不默认作为运行时依赖。

不适合作为直接依赖的内部模块：

- `codex-core::unified_exec::*` 是 `pub(crate)`，而且耦合 Codex session、approval、sandbox、tool orchestration、network approval。它适合作为行为蓝本和局部代码移植来源，不适合作为 AgentDash 运行时依赖。
- `HeadTailBuffer` 当前也是 `codex-core` 私有模块；可以按许可和项目风格移植为 AgentDash 自有 bounded buffer，或用 `codex-utils-output-truncation` 补最终文本截断。

## Data Flow

1. 模型调用 shell start 工具。
2. `agentdash-application` 解析 mount/cwd，生成 relay command。
3. `BackendRegistry` 只等待 start RPC 的初始窗口响应。
4. `agentdash-local` 创建 shell session，输出 watcher 写入 retained buffer，并通过 relay event 推送 delta。
5. 初始 `yield_time_ms` 到达时，若进程仍运行，返回 running result 和 `session_id/process_id`。
6. 模型或服务端后续调用 shell read/input/terminate。
7. 进程退出后，exit watcher 发送 state_changed，并在 retention 窗口内允许读取尾部输出。
8. 前端 command card 与 terminal tab 订阅同一个 session id 的 output/state projection。

## Trade-offs

直接依赖 `codex-exec-server` 能减少 process protocol 和 read/write/terminate 细节实现量，但会带入较多 Codex 相关依赖、远程环境抽象和沙盒周边能力；这些能力并不是 AgentDashboard 当前 shell 工具扩展的目标。

在 AgentDash 内部移植/重写 manager 能保持依赖轻、边界贴合 relay/VFS/terminal_cache，但需要我们自己维护 spawn、read wait、retention、prune 和截断测试。

确认方案是轻量复用路线：升级 Codex 依赖到 `rust-v0.140.0`，优先直接使用 `codex-utils-pty` / `codex-utils-output-truncation` 这类小而公开的 crate；`codex-exec-server` 仅作为协议和行为参考，不默认纳入依赖。`ShellSessionManager` 在 `agentdash-local` 内按 AgentDash relay/VFS/terminal 边界实现，必要时移植 Codex 的基础 buffer / wait 细节，但不引入 Codex 沙盒、approval、network approval 或 session orchestration。

实现时不保留并行能力：如果 `ShellSessionManager` 或轻量 Codex crate 已经承接 process spawn、PTY、输出截断、read wait 或 terminal state，就删除 AgentDash 过去自维护的对应旧路径。项目处于预研阶段，正确的单一事实源优先于兼容式双轨。

## Migration

项目未上线，协议迁移按一次性收束处理：

- relay DTO、application adapter、local handler、frontend generated types 同步更新。
- 旧 `shell_exec` 响应结构不保留兼容分支。
- 数据库不需要持久化 shell output；terminal/session 状态仍按运行时内存 projection 处理。
- Codex git dependency 升级和新增 Codex crate dependency 与 shell 工具实现同任务验证。
