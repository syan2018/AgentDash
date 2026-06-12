# 平台 shell exec 原语执行计划

## Checklist

- [ ] 读取相关规范：backend VFS architecture/access/materialization、capability tool pipeline、runtime gateway、cross-layer desktop local runtime。
- [ ] 定位现有 `shell_exec` 分派、VFS tool factory、CapabilityState tool policy、runtime VFS assembly 注入点。
- [ ] 设计 platform shell executor 模块位置与 public API。
- [ ] 调整 `ShellExecTool`：保存 platform shell 所需 capability context；`cwd` 缺失或 `platform://` 时进入 platform shell；显式 exec mount 继续现有路径。
- [ ] 实现 platform shell parser：单行、基础 quoting、窄重定向、明确拒绝 unsupported syntax。
- [ ] 实现 path resolver：VFS URI 与平台 cwd-relative path。
- [ ] 实现 command handlers：`pwd`、`ls`、`cat`、`cp`、`mv`、`rm`、`echo`。
- [ ] 在每个 command handler 内执行 `file_read` / `file_write` 与 mount capability 校验。
- [ ] 更新 `shell_exec` tool description，说明默认平台 shell 和显式 cwd 的 OS shell 行为。
- [ ] 增加 focused tests 覆盖默认平台 shell、显式 OS shell 分派、lifecycle copy、权限拒绝和 unsupported syntax。
- [ ] 如实现改变 runtime VFS mount projection 或 tool policy contract，更新相关 spec。

## Validation Commands

根据实际改动选择精简验证：

```powershell
pnpm run backend:clippy
```

可先跑更窄的 Rust 测试，例如：

```powershell
cargo test -p agentdash-application platform_shell
cargo test -p agentdash-application shell_exec
```

提交前至少运行：

```powershell
pnpm run migration:guard
git diff --check
```

## Risky Files

- `crates/agentdash-application/src/vfs/tools/fs/shell.rs`
- `crates/agentdash-application/src/vfs/tools/factory.rs`
- `crates/agentdash-application/src/vfs/service.rs`
- `crates/agentdash-application/src/capability/*`
- `crates/agentdash-spi/src/platform/tool_capability.rs`
- lifecycle VFS provider tests if artifact copy coverage needs fixture support

## Review Gates

- Confirm no platform shell path routes unsupported command to OS shell implicitly.
- Confirm explicit `cwd` to normal exec mount still exercises existing materialization behavior.
- Confirm platform shell write operations cannot bypass `file_write` or lifecycle writable port constraints.
- Confirm user-facing tool description does not imply full bash support.
