# Extension Runtime 收口维护 Design

## Architecture Intent

这项维护任务不改变 extension runtime 的产品方向，而是把当前 MVP 的横切规则放回稳定 owner：权限归 evaluator，archive bytes 归 storage port，local TS host 归 extension host 子系统。目标是让后续增加 workspace/VFS/HTTP/process/env permission、更多 webview 类型或 extension assets 前端管理时，不再继续复制规则。

## Boundary 1: Permission Evaluator

建议新增 application/domain 可共享的 evaluator 模型。它不应该只返回 bool，而应返回可审计的裁决：

```text
ExtensionPermissionDecision
  allowed: bool
  requested_permission: string
  extension_key / extension_id
  action_key
  reason_code
```

首版输入可以保持简单：

```text
ExtensionTemplatePayload
ExtensionRuntimeActionDefinition
requested permission key
```

`local.profile.read` 的规则采用双重满足：

- extension manifest 顶层 `permissions` 包含 `{ kind: "local_profile", access: "read" | "read_write" }`
- runtime action `permissions` 包含 `"local.profile.read"`

Gateway provider 调用 evaluator 来决定是否 relay 到 local backend。Local host 的 `local.get_profile` host API 也调用同一 evaluator 或同一 domain helper。Projection 可以继续暴露 manifest 中的 raw permissions，但 invocation metadata 应附带 resolved permission decision 的关键字段，避免审计只能从错误字符串反推。

## Boundary 2: Artifact Storage Port

当前 artifact validation、digest 和 install orchestration 属于 application；filesystem-backed object read/write 属于 infrastructure。推荐拆成：

```text
agentdash-application
  extension_package.rs
    validate archive
    store artifact use case
    install artifact use case
    read archive / webview asset use case
    consumes ExtensionPackageArtifactStorage port

agentdash-spi
  extension_package.rs
    trait ExtensionPackageArtifactStorage

agentdash-infrastructure
  persistence/postgres/extension_package_artifact_repository.rs
  storage/extension_package_artifact_fs.rs
```

Storage port 负责：

- `put_archive(storage_ref, bytes)`
- `get_archive(storage_ref)`
- storage ref path normalization
- storage root 选择
- atomic write / read error mapping

Application use case 负责：

- archive digest 与 manifest digest
- package manifest validation
- bundle digest validation
- artifact row upsert
- Project installation upsert
- webview asset path allowlist 与 archive file extraction

API route 只调用 use case，不直接调用 object read/write。这样 route 层不会拥有 storage 行为，也不会在 `extension_runtime` route 与 `extension_package_artifacts` route 中复制 digest 校验和 archive read 逻辑。

## Boundary 3: Local Extension Host Modules

当前 `extensions/host.rs` 已经承载了太多职责。推荐目标布局：

```text
crates/agentdash-local/src/extensions/
  mod.rs
  artifact_cache.rs
  host/
    mod.rs
    manager.rs
    process.rs
    protocol.rs
    permissions.rs
    runner.rs
```

职责划分：

- `manager.rs`：公开 `LocalExtensionHostManager`，管理 activate/reload/invoke/health 生命周期。
- `process.rs`：Node runner 进程启动、stdin/stdout request-response、退出重置。
- `protocol.rs`：runner request/response、host api request/response DTO。
- `permissions.rs`：local host 侧调用 shared evaluator 的薄封装。
- `runner.rs`：内嵌 JS runner 字符串和生成/写入逻辑；如果后续改外部 runner 文件，也只影响这里。

`handlers/extension.rs` 可以先保持单文件，除非本任务实现过程中发现 handler 也需要拆成 invoke/artifact/cache 子模块。

## Trust Model Note

本任务不强制把 Node `vm` 变成真正 sandbox。当前更合理的短期语义是 trusted local extension：

- 本机安装和运行的 extension 与本机用户处在同一信任域。
- Host API facade 用于产品权限、审计、可解释 contract，不宣称 OS/Node 级隔离。
- 若后续要进入 isolated execution，需要单独任务处理 worker/process 隔离、realm 函数注入、escape test、runner package 分发等问题。

但即使是 trusted 模型，Gateway 与 local host 也必须统一权限语义，因为这是平台产品权限和审计的基础。

## Data Flow After Convergence

```text
Webview bridge / Canvas runtime
  -> API extension-runtime invoke route
  -> RuntimeGateway ExtensionRuntimeActionProvider
  -> ExtensionPermissionEvaluator
  -> relay command.extension_action_invoke
  -> agentdash-local handler
  -> LocalExtensionHostManager
  -> host permissions wrapper / evaluator
  -> TS extension action
```

Artifact path:

```text
SDK pack / Canvas promote
  -> API upload/promote route
  -> application extension package use case
  -> infrastructure artifact storage adapter
  -> extension_package_artifacts row
  -> ProjectExtensionInstallation package ref
```

Webview asset path:

```text
WorkspacePanel iframe src
  -> API webview asset route
  -> application read webview asset use case
  -> infrastructure artifact storage adapter
  -> digest check
  -> declared panel directory allowlist
  -> static bytes response
```

## Migration And Compatibility

如果 storage port 只是移动代码和注入依赖，不需要新增数据库 migration。若实现过程中调整 artifact metadata 字段，应新增 migration，并同步 Postgres repository、domain validation、contract generation 与 frontend mapper。

当前项目处于预研期，不需要保留旧 API/字段兼容层；如果 schema 变化能让模型更正确，应直接迁移到正确形态。

## Trade-Offs

- 先抽 evaluator 比直接扩全量 permission 更小，但能阻止 Gateway/local drift 继续扩大。
- Storage port 会增加一点 bootstrap wiring，但能让 archive download、webview asset read、Canvas promote 和 install 共用边界。
- 拆 `host.rs` 不直接新增用户功能，但能避免 extension host 后续成为维护瓶颈。
