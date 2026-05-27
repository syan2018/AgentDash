# Extension Artifact Storage 边界抽离 Design

## Boundary

当前目标是消除 route-to-route 业务依赖。最小方案是在 application 层新增 extension artifact archive storage helper/service，API route 通过该服务读写 archive object。后续如果需要多 storage backend，再把具体 filesystem 实现下沉到 infrastructure adapter。

## Proposed Shape

候选模块：

```text
crates/agentdash-application/src/extension_package.rs
```

在该模块中放置：

- storage root 解析。
- storage ref 到 filesystem path 的安全解析。
- archive bytes read/write。
- object path 生成。

API route 调整：

- `extension_package_artifacts.rs` 调用 application helper 写 archive。
- `extension_runtime.rs` 调用 application helper 读 archive/webview asset 所需 archive bytes。
- route 间不再互相 import helper。

## Safety

- storage ref 必须保持相对路径，不接受 absolute path、parent dir、root/prefix component。
- write 前创建 parent directory。
- read/write 不改变 archive digest 校验的事实源；digest 仍由现有 package artifact use case 校验。

## Validation

```powershell
cargo check -p agentdash-api
cargo test -p agentdash-api extension
```

如果 API crate 测试目标过宽或无匹配测试，则至少运行 `cargo check -p agentdash-api`。
