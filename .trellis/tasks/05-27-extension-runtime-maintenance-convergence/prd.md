# Extension Runtime 收口维护

## Goal

把当前 TypeScript Extension Runtime MVP 从“端到端链路可跑通”收口成后续可持续扩展的维护基线：权限裁决有单一 owner，package artifact storage 有清晰 application / infrastructure 边界，`agentdash-local` extension host 子系统不再继续堆在单个大文件里。

## Background

当前 `codex/extension-sdk` 分支已经形成 Project scoped extension installation、packaged archive 上传与安装、RuntimeGateway extension proxy、本机 TS Extension Host、WorkspacePanel dynamic tab、webview bridge，以及 `@agentdash/extension-sdk` / `@agentdash/extension-ui` / `@agentdash/extension-dev` 作者工具链。整体方向符合后续 TS Extension Host + SDK MVP：云端保存 Project 安装事实和 artifact，本机 local runtime 执行 TS extension，前端只通过 runtime projection 与 bridge 调用平台能力。

Review 发现还有三个横切边界没有真正收口：

- 权限语义在 Gateway、local host 和 projection/audit 之间仍有漂移风险。Domain 已有 `allows_local_profile_read_for_action` 这类“双重满足”规则，但 Gateway admission 和 local host enforcement 仍各自解释权限。
- extension package archive 的 filesystem object IO 已经从 API route helper 中挪出，但仍以 application free function 形式直接处理 storage root、path、read/write；还没有形成 infrastructure adapter owner。
- `agentdash-local/src/extensions/host.rs` 承载 manager、process lifecycle、IPC、host API、权限判断、内嵌 JS runner 和测试；目录化已经开始，但 extension host 内部模块所有权还不够清晰。

## Requirements

### R1. Extension permission evaluator 收口

- 定义 extension runtime permission evaluator，至少覆盖 `local.profile.read` 的当前语义。
- Gateway admission、local host host-api enforcement、projection/audit metadata 必须通过同一规则或同一 contract fixture 得出一致结果。
- `local.profile.read` 必须满足 extension 顶层 `local_profile` grant 与 action-level `local.profile.read` usage declaration。
- 未知 permission 不允许被默认放行；未来 workspace/http/process/env permission 要能在同一 evaluator 模型中扩展。

### R2. Package artifact storage owner 收口

- 把 archive object read/write、storage root、storage ref path normalization 从 application free function 抽成明确 storage port。
- infrastructure 层实现 filesystem-backed storage adapter。
- application use case 负责 archive validation、digest、artifact record、install orchestration、webview asset read 意图。
- API route 只保留 auth、request parsing、DTO mapping、error mapping，不直接知道 filesystem storage path。
- archive download、webview asset read、Canvas promote package、Project install 复用同一 artifact storage service。

### R3. Local extension host 模块拆分

- `agentdash-local` 中 extension host 相关代码继续保留在 `extensions/` 子系统下，但拆出 manager、protocol、runner、permissions 等内部模块。
- relay command handler 与 extension execution dependency 分层清晰：handler 负责命令解析/响应，extension host 负责 activate/invoke/health，artifact cache 负责下载校验和解包缓存。
- `lib.rs` 只 re-export 稳定入口，不暴露内部文件布局。

### R4. 保持现有正确产品边界

- Project enabled extension installations 仍是 runtime projection 的事实源。
- Packaged extension artifact 仍是正式安装与 Canvas promote 的共同运行产物。
- 云端不执行 TS extension，本机 local runtime 承接 TS host 执行。
- 前端 webview 仍只能通过 `@agentdash/extension-ui` bridge 调用平台能力，不获得主前端 store/token/内部对象。

## Acceptance Criteria

- [ ] Gateway 与 local host 对 `local.profile.read` 使用同一 evaluator 或同一组 fixture；“extension 顶层声明但 action 未声明”的 action 在 Gateway admission 与 local host enforcement 都被拒绝。
- [ ] evaluator 覆盖：顶层无/action 有、顶层有/action 无、二者都有、未知 permission 四类用例。
- [ ] Runtime invocation metadata 或 projection 中能够表达权限裁决所依据的 extension/action 身份，不让审计只能靠 local host 错误字符串推断。
- [ ] package artifact object IO 经由 infrastructure-backed storage adapter；API route 不直接调用 filesystem read/write helper。
- [ ] webview asset read 与 archive download 复用同一 artifact storage use case，并继续校验 archive digest。
- [ ] Canvas promote 产出的 packaged artifact 与 SDK upload artifact 走同一 validation/storage/install 边界。
- [ ] `agentdash-local/src/extensions/host.rs` 被拆分为清晰子模块，后续新增 VFS/HTTP/env/process 权限时有明确归属。
- [ ] 相关 Rust 单元测试、前端 extension runtime mapper/bridge 测试、`@agentdash/extension-dev` 测试和关键 E2E 命令在 `implement.md` 中列出；依赖缺失只记录为环境准备问题。

## Out Of Scope

- 不扩展 user-level/global extension installation。
- 不实现完整 marketplace、签名分发或第三方信任生态。
- 不一次性补齐 workspace/http/process/env/VFS 全量 permission 行为，只为这些能力预留 evaluator 扩展形状。
- 不重做 WorkspacePanel 产品形态；前端只跟随 contract 或权限/信任状态需要做最小更新。
- 不为旧 manifest / 旧数据库字段增加兼容层；当前项目仍按预研期正确形态推进，并通过 migration 处理 schema 变化。

## Open Questions

- TS Extension Host 首版是否继续按 trusted local extension 明示，还是在本任务直接切到更强 isolated worker/process 模型？建议本任务先把 trusted 状态如实表达并把权限/审计收口，isolated execution 作为后续安全加固任务，除非实现中发现当前 runner 已经无法承载权限语义。
