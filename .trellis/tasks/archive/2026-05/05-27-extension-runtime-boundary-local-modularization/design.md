# Extension Runtime 边界收口与 local 模块目录化 Design

## Architecture Intent

这项优化要把 extension runtime 的横切边界归位：安全与信任模型归 host contract，权限归 shared evaluator，artifact bytes 归 storage service，HTTP route 只保留 interface 职责，本机执行代码按 extension 子系统聚合。这样后续增加 workspace read/write、runtime action delegation 或更多 webview 类型时，不会让规则继续散落在 API route、RuntimeGateway、本机 handler 和前端投影中。

## Workstream 1: Host Trust Boundary

当前 Node `vm` 只能作为模块加载与全局对象控制手段，不能作为不受信代码安全边界。后续设计必须选择并写明其中一种模型：

- `trusted`：本机安装的 extension 与本机用户同信任域，Host API facade 用于产品权限、审计和契约一致性，不宣称 OS/Node 级隔离。
- `isolated`：插件代码在独立进程、worker 或更强隔离单元中执行；host 与 runner 通过显式 IPC 交换 activate/invoke/health 消息；文件、HTTP、env、process、workspace 等能力只能经 host API facade 请求。

如果采用 isolated 模型，runner 不应向插件 context 注入来自宿主 realm 的函数对象；计时器、structured clone、console 等基础能力需要来自隔离 realm 或由协议代理。安全验收以“插件无法获得 Node `process` 或任意 `require/import` 宿主能力”为准。

## Workstream 2: Permission Evaluator

权限语义要形成一个可复用 contract，而不是 Gateway 和 local host 各自解释 manifest。

建议抽象：

- 输入：extension manifest snapshot、runtime action declaration、requested host API capability、actor/context placement。
- 输出：allow/deny、denial reason、用于审计的 resolved permission metadata。
- 规则：host API 使用必须满足 extension-level capability 与 action-level usage declaration 的统一语义；具体是“双重满足”还是“顶层授权覆盖 action”由本任务的 open decision 落定，但 Gateway、projection 和 local host 必须一致。

测试 fixture 应覆盖：

- extension 顶层有 `local_profile`，action permissions 为空。
- extension 顶层无 `local_profile`，action permissions 有 `local.profile.read`。
- extension 与 action 均声明。
- 非 local profile 的未知 permission 或未来 workspace permission 不被误放行。

## Workstream 3: Artifact Storage Ownership

Artifact storage 不应归 API route。推荐将 archive storage 拆成明确端口：

- application 层 use case 负责 package artifact 校验、digest、metadata、install 编排与读取授权后的 archive/webview asset 意图。
- infrastructure 层实现 filesystem-backed storage adapter，负责 storage root、object path、atomic write/read、path normalization。
- API route 注入 application service，只做 project auth、request parsing、response mapping 与 error mapping。

这样 `extension_package_artifacts` route 与 `extension_runtime` route 都调用同一 service，而不是 route-to-route import helper。Canvas promote 继续产出同款 packaged artifact，并走相同 storage service。

## Workstream 4: Local Crate Module Layout

`agentdash-local` 目前已有 `handlers/` 目录，但 extension host 与 artifact cache 仍在 crate 根目录。后续目录化建议先收敛 extension 子系统，不触碰无关 runtime 文件。

候选布局：

```text
crates/agentdash-local/src/
  extensions/
    mod.rs
    artifacts.rs
    host/
      mod.rs
      manager.rs
      permissions.rs
      protocol.rs
      runner.rs
  handlers/
    extension/
      mod.rs
      invoke.rs
      artifacts.rs
```

如果实际代码规模不支持拆到这么细，可以先采用较小布局：

```text
crates/agentdash-local/src/
  extensions/
    mod.rs
    artifact_cache.rs
    host.rs
  handlers/
    extension.rs
```

选择标准是模块所有权：artifact cache 属于 packaged extension execution dependency，host manager/runner/protocol 属于 extension host，relay command parsing 属于 handlers。`lib.rs` 只 re-export `LocalExtensionHostManager`、`LocalTsExtensionHostConfig`、`download_and_cache_extension_artifact` 等稳定入口。

## Data Flow

```text
WorkspacePanel / webview bridge
  -> API extension-runtime invoke route
  -> RuntimeGateway dynamic extension provider
  -> relay command.extension_action_invoke
  -> agentdash-local handlers::extension
  -> extension artifact cache
  -> local TS extension host activate/invoke
  -> relay response.extension_action_invoke
  -> API response / webview bridge
```

Webview asset read follows the same Project installation and artifact storage authority:

```text
WorkspacePanel tab descriptor
  -> API extension-runtime webview asset route
  -> application artifact storage service
  -> validated package artifact object
  -> static asset response
```

## Trade-Offs

- 直接做 isolated execution 成本更高，但能避免把临时 trusted 模型沉淀成错误安全承诺。
- 先 trusted 明示成本低，适合快速验证 SDK；代价是 UI/contract 需要真实表达风险，后续切 isolated 时要补强迁移说明。
- 一次性移动整个 `agentdash-local` 风险较大；只移动 extension 子系统更利于 review，也能立刻解决本次增长点。

## Validation Strategy

- Rust unit tests 覆盖 permission evaluator、RuntimeGateway admission、local host enforcement 与 artifact storage service。
- Node/runner test 覆盖 host context escape 用例；如果选择 trusted，则测试应改为验证 contract 不宣称 sandbox，而不是假装隔离。
- API route tests 覆盖 archive download/webview asset read 不依赖 route-local helper。
- `cargo check -p agentdash-local` 验证 local module move 后 re-export 和 handler 引用正确。
- 前端 extension runtime tests 验证 dynamic tab 与 webview bridge 没有因 contract 收口漂移。
