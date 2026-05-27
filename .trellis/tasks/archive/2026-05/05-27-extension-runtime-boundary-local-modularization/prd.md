# Extension Runtime 边界收口与 local 模块目录化

## Goal

把当前 Project scoped TypeScript Extension 闭环从“能跑通 demo 的链路”收口成可持续演进的运行底座：明确 TS Extension Host 的信任边界，统一 RuntimeGateway 与本机 host 的权限裁决，抽离 extension artifact storage 的模块归属，并整理 `agentdash-local` 中 extension 相关代码的目录结构。

## Background

当前分支已经形成 extension manifest/runtime contract、packaged artifact 上传与安装、RuntimeGateway 动态 provider、本机 TS Extension Host 执行、WorkspacePanel 动态 tab，以及 `@agentdash/extension-sdk` / `@agentdash/extension-ui` / `@agentdash/extension-dev` 作者侧工具链。整体方向成立：云端保存 Project 安装事实与 artifact，本机承载 TS 插件执行，前端只消费 runtime projection 和 webview/canvas tab。

Review 发现需要进入后续优化的风险：

- TS Extension Host 目前使用 Node `vm` 加载 bundle，并向 context 注入宿主函数；这不是可靠 sandbox，插件代码可通过宿主函数构造器拿到 Node `process`，与“Host API facade 才是能力入口”的目标不一致。
- extension action 权限在 Gateway 和本机 host 两端语义不同：Gateway 只看 action permissions，本机 host 可接受 extension 顶层 permission；这会让投影、审计和实际能力漂移。
- extension artifact filesystem helper 当前位于 API route，且另一个 route 反向 import 这些 helper；这让 route 层承载了 storage 业务依赖。
- `crates/agentdash-local/src` 中 extension host、artifact cache、runtime、tool、workspace 等文件整体偏平铺；extension runtime 继续增长后会降低模块所有权清晰度。

## Requirements

- 明确 TS Extension Host 首版信任模型。
  - 如果目标是受限执行，必须移除 Node `vm` 作为安全边界的假设，改为独立进程/worker 与显式 IPC，并验证插件不能拿到宿主 Node 全权限对象。
  - 如果首版接受本机插件为 trusted extension，contract、UI 与文档必须如实表达 trusted，不把当前 `vm` 称为安全 sandbox。
- 收敛 extension runtime 权限语义。
  - Host API 调用、Gateway admission、runtime projection 与审计 metadata 必须使用同一套 permission evaluator 或同一组 contract fixture。
  - `local.profile.read` 等 host API 权限必须同时能够表达 extension 顶层 capability 与 action-level 使用声明之间的关系。
- 把 extension package artifact storage 从 API route 抽离。
  - API route 只负责鉴权、DTO、调用 application service 和错误映射。
  - artifact archive 的读写、路径解析、digest 校验与 storage ref 解释归 application service / infrastructure adapter 边界。
  - route 之间不能 import storage helper 形成业务依赖。
- 将 `agentdash-local` 的 extension runtime 相关代码目录化。
  - extension host runner、protocol、permission、artifact cache、relay command handler 的归属应按模块目录表达。
  - crate 对外 re-export 保持清晰入口，避免让调用方依赖内部文件布局。
  - 目录化优先处理 extension 相关代码，不扩大成整个 local crate 的无差别重排。
- 保持现有正确方向。
  - Project enabled extension installations 仍是 runtime projection 的事实源。
  - Packaged extension artifact 仍是正式安装与 Canvas promote 的共同运行产物。
  - 云端不执行 TS 插件；本机 runtime 通过 relay 承接执行。

## Acceptance Criteria

- [ ] TS Extension Host 的 contract 明确区分 trusted 与 isolated execution；若采用 isolated execution，测试覆盖插件不能通过注入对象拿到 Node `process`。
- [ ] `local.profile.read` 至少覆盖一个“extension 顶层声明但 action 未声明”的拒绝用例，Gateway 与本机 host 行为一致。
- [ ] extension permission evaluator 有共享测试 fixture 或复用实现，RuntimeGateway、local host 与 runtime projection 对同一 manifest/action 得出一致结论。
- [ ] `agentdash-api/src/routes` 不再拥有 extension artifact filesystem storage helper，且 `extension_runtime` route 不再从 `extension_package_artifacts` route import storage helper。
- [ ] extension artifact storage 有清晰的 application/infrastructure 边界，archive download、webview asset read 与 package install 复用同一归属。
- [ ] `crates/agentdash-local/src` 中 extension 相关文件移动到模块目录，建议形成 `extensions/host`、`extensions/artifacts`、`handlers/extension` 等边界，`lib.rs` 只 re-export 稳定入口。
- [ ] 相关 Rust 测试、前端 extension runtime 测试、contract check 与 extension-dev 测试有明确验证命令；依赖缺失时记录需要先安装依赖，而不是把断言失败和环境问题混在一起。

## Out Of Scope

- 不扩展成完整 marketplace 或 user-level/global extension 体系。
- 不为旧 manifest 字段、旧 API 或旧数据库形态增加兼容层。
- 不一次性实现所有未来权限种类；本任务先收敛当前 Project scoped packaged extension、workspace tab、runtime action 与 local profile 相关权限。
- 不重写 WorkspacePanel 或 extension SDK 的产品形态；前端只在权限/信任提示或 contract 变化需要时跟进。

## Open Decision

首版 TS Extension Host 应该按 trusted extension 明示，还是直接实现真实隔离执行？推荐方向是直接把 contract 按真实目标设计为 isolated execution，并允许 implementation 分阶段落地；如果短期继续 trusted，必须把 trusted 状态显式投影给开发者与使用者。
