# Extension Host 信任模型与权限裁决收口 Design

## Trust Model

本任务将当前 TS Extension Host 明确为 trusted local extension runner。原因是当前 Node `vm` 加载 self-contained ESM bundle 时仍运行在宿主 Node 进程的安全域内，不能提供对不受信插件的可靠隔离。Host API facade 在本阶段承担产品权限、审计、协议稳定性和后续隔离迁移的入口，而不是 OS/Node 级安全边界。

实现上保留 Node host 子进程与 stdio JSON line 协议。代码、spec 和错误信息避免把 `vm` 称为 sandbox；如果保留 context 限制，应命名为 runner context 或 module context。

## Permission Semantics

本任务采用严格语义：

```text
host API is allowed
  = extension manifest grants top-level capability
  AND current action declares permission usage
```

对 `api.local.getProfile()`：

- 顶层 permission 需要声明 `local_profile` read/read_write。
- 当前 action permissions 需要包含 `local.profile.read`。
- 任一缺失时拒绝，并返回可审计的 permission denied 错误。

Gateway admission 与 local host enforcement 使用同一语义，但触发时机不同。Gateway 能校验 action 已声明的 host API permission 是否被 extension 顶层 capability 覆盖；local host 在插件实际调用 host API 时使用当前 action key 做最终 enforcement。因此 action 未声明 `local.profile.read` 时，Gateway 不把它投影为 profile 读取能力，本机 host 也不会允许它实际读取 profile。

## Implementation Shape

优先寻找现有 manifest/action 类型并抽取小 helper，而不是引入过大的权限框架。候选位置：

- application 侧：`agentdash-application::runtime_gateway::extension_actions` 附近放 admission helper。
- local 侧：`agentdash-local` extension host 模块内放 host API enforcement helper。
- 如果有共享 contract/domain 类型可复用，可以把纯判定函数下沉到更内层 crate；但不要为了单个 permission 过早扩张 crate 依赖。

测试必须覆盖同一语义，而不是只测某一端：

- Gateway：action permission 包含 `local.profile.read` 但顶层 capability 缺失时拒绝。
- Local host：manifest 顶层有 local profile 但 action permission 为空时，插件调用 `ctx.api.local.getProfile()` 被拒绝。
- Existing positive case：顶层和 action 都声明时允许读取 profile。

## Local Module Directory

如果需要编辑 `extension_host.rs` 大块逻辑，则同步做最小目录化：

```text
crates/agentdash-local/src/
  extensions/
    mod.rs
    host/
      mod.rs
      permissions.rs
```

本任务不强制拆 runner 大字符串。若拆 runner 会显著扩大 diff，可先只把权限 helper 拆入 `extensions/host/permissions.rs`，并保留后续拆 runner/protocol 给 parent task。

## Validation

```powershell
cargo test -p agentdash-local extension_host
cargo test -p agentdash-application extension_actions
cargo check -p agentdash-local
```

如果 contract/spec 文案更新，补充运行与改动相关的 check；本任务不要求前端全量测试。
