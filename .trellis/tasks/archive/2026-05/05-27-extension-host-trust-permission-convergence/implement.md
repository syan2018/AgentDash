# Extension Host 信任模型与权限裁决收口 Implement Plan

## Step 1: Read Context

- 读取 parent task 的 `prd.md`、`design.md`、`implement.md`。
- 读取本任务 `prd.md` 和 `design.md`。
- 读取 Trellis specs：
  - `.trellis/spec/backend/architecture.md`
  - `.trellis/spec/backend/runtime-gateway.md`
  - `.trellis/spec/cross-layer/desktop-local-runtime.md`
  - `.trellis/spec/cross-layer/frontend-backend-contracts.md`
  - `.trellis/spec/guides/cross-layer-thinking-guide.md`

## Step 2: Inspect Current Code

- `crates/agentdash-local/src/extension_host.rs`
  - 找到 `allows_local_profile` 与 host API permission denied 测试。
  - 找到 Node runner/context 文案与错误信息。
- `crates/agentdash-application/src/runtime_gateway/extension_actions.rs`
  - 找到 action permission admission 逻辑。
  - 确认 extension-level permission 是否已进入 provider 判定上下文。
- 相关 manifest/permission 类型所在 domain/contracts 文件。

## Step 3: Implement Permission Convergence

- 将 local profile 判定改为 extension 顶层 capability 与 action permission 同时满足。
- Gateway admission 使用同一语义或增加等价测试，保证 action permission 为空时拒绝。
- 保持错误类型与返回路径符合现有模式，不引入兼容旧语义。

## Step 4: Clarify Trust Model

- 更新 `desktop-local-runtime.md` 中 Local TS Extension Host 描述，明确当前是 trusted local extension runner，不宣称 Node `vm` 是安全 sandbox。
- 代码注释/错误信息如有 sandbox 描述，同步改名。

## Step 5: Optional Local Directory Move

- 如果 Step 3 需要拆权限 helper，将其放入 `crates/agentdash-local/src/extensions/host/permissions.rs`。
- 更新 `lib.rs` 与模块引用。
- 不移动 artifact cache 与 runner/protocol，除非它们已成为实现阻碍。

## Step 6: Verify

```powershell
cargo test -p agentdash-local extension_host
cargo test -p agentdash-application extension_actions
cargo check -p agentdash-local
```

记录任何未能运行的验证及原因。
