# Runtime VFS 工具执行链实施计划

## Phase 1. 固定失败回归

- [x] 在 `agentdash-application-vfs` 增加显式 `main://...` patch 经 non-native provider 执行的失败测试。
- [x] 覆盖 Add、Update、Delete、Move header 与 patch body URI 字面量。
- [x] 在 authorization 测试中复现显式 cwd 的 Exec grant 与 command URI 的 Read 需求。
- [x] 增加 PowerShell here-string 数据区测试。

验证：

```powershell
cargo test -p agentdash-application-vfs apply_patch -- --nocapture
cargo test -p agentdash-application-vfs rewrite -- --nocapture
cargo test -p agentdash-infrastructure runtime_tool_authorization -- --nocapture
```

## Phase 2. 统一 patch provider 边界

- [x] 删除 native/non-native 分支各自解释 patch path 的差异，保留一个单 mount header/entry 归一化入口。
- [x] inline、composite、native provider 统一消费 mount-relative path。
- [x] 保持 multi-mount 分组、Move 源/目标校验和 policy admission。
- [x] 保留路径、权限、provider 编辑能力错误边界。

验证：

```powershell
cargo test -p agentdash-application-vfs apply_patch -- --nocapture
cargo test -p agentdash-application-vfs service -- --nocapture
```

## Phase 3. 收束 Shell URI 扫描

- [x] authorization、materialization 与 unresolved guard 共用 shell URI scanner。
- [x] 固定普通参数、带引号参数和 PowerShell here-string 的最小词法规则。
- [x] authorizer 从共享候选派生 Exec/Read grant。
- [x] materializer 复用同一候选语义与 replacement span。
- [x] 复用既有 grant merge，未增加新的资源模型或工具 schema。
- [x] continuation 分支未修改。

验证：

```powershell
cargo test -p agentdash-application-vfs materialization -- --nocapture
cargo test -p agentdash-application-vfs shell_exec -- --nocapture
cargo test -p agentdash-infrastructure runtime_tool_authorization -- --nocapture
```

## Phase 4. 跨层终态验证

- [x] 核对“relay 下发前失败”的服务端工具终态事件。
- [x] 验证 `sessionStreamReducer` 对 `item_completed` 收敛为 terminal 且停止 streaming。
- [x] 确认失败发生在 terminal registration 前，不会产生 terminal store 条目。
- [x] 无前端复现证据，不实施补偿修改。

验证命令按现有 package script 选择定向 Vitest：

```powershell
pnpm --filter app-web test -- sessionStreamReducer
pnpm --filter app-web test -- useTerminalStore
```

## Phase 5. 规范与质量门

- [x] 更新 VFS materialization 规范，记录 provider-relative patch 与共享 shell URI scanner 契约。
- [x] 使用 `rustfmt --edition 2024 --config skip_children=true` 定向格式化本任务 Rust 文件。
- [x] 定向回归测试通过；后续 workspace check 被并行会话未完成的 `wire_u64::option` 引用阻断，未修改该文件。
- [x] 没有前端改动，不运行前端 typecheck。
- [x] 核对 git diff，未触碰 Canvas 和并行会话文件。

建议质量命令：

```powershell
cargo check -p agentdash-application-vfs
cargo check -p agentdash-infrastructure
cargo test -p agentdash-application-vfs
cargo test -p agentdash-infrastructure runtime_tool_authorization -- --nocapture
pnpm --filter app-web typecheck
```

## 预计修改范围

- `crates/agentdash-application-vfs/src/service.rs`
- `crates/agentdash-application-vfs/src/apply_patch.rs`
- `crates/agentdash-application-vfs/src/rewrite.rs`
- `crates/agentdash-application-vfs/src/materialization.rs`
- `crates/agentdash-application-vfs/src/tools/fs/shell.rs`
- `crates/agentdash-infrastructure/src/runtime_tool_authorization.rs`
- 对应测试模块
- 仅在复现后涉及 `packages/app-web` 的 reducer/store 测试或最小修复
- `.trellis/spec/backend/vfs/*`

## 提交建议

```text
fix(vfs): 统一 Runtime 工具路径授权与物化契约

- 归一化 apply patch 的 provider 相对路径
- 统一 shell 资源计划、授权与物化边界
- 补齐失败终态与脚本数据边界测试
```
