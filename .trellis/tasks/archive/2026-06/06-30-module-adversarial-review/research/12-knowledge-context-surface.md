# Research: Knowledge & Context Surface

- Query: 单域对抗性架构审查：Knowledge & Context Surface
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/tasks/06-30-module-adversarial-review/check.jsonl` - 本轮审查上下文与相关 spec/baseline 读取记录。
- `.trellis/tasks/06-30-module-adversarial-review/prd.md` - 对抗性模块审查目标与输出约束。
- `.trellis/tasks/06-30-module-adversarial-review/design.md` - 审查方法、baseline 对照与模块边界。
- `.trellis/tasks/06-30-module-adversarial-review/implement.md` - 分域审查执行计划。
- `.trellis/spec/cross-layer/shared-library-contract.md` - Shared Library / Project Asset / runtime projection 的所有权契约。
- `.trellis/spec/backend/session/architecture.md` - FrameConstruction / AgentFrame / RuntimeSession 的边界契约。
- `.trellis/spec/backend/capability/architecture.md` - capability resolver、dimension replay、runtime capability surface 规则。
- `.trellis/spec/backend/vfs/architecture.md` - VFS final surface 与 runtime tool/skill projection 规则。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` - 06-14 baseline 总览。
- `.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md` - 06-14 AgentRun/session runtime baseline。
- `.trellis/tasks/06-14-module-overdesign-review/research/03-vfs-local-relay-extension.md` - 06-14 VFS/local relay/extension baseline。
- `crates/agentdash-application-skill/src/skill_asset/service.rs` - Project SkillAsset CRUD、builtin seed、remote skill import materialization。
- `crates/agentdash-application/src/skill_asset/mod.rs` - remote skill URL import facade，连接 Shared Library install 到 Project SkillAsset。
- `crates/agentdash-application-vfs/src/mount_skill_asset.rs` - Project skill asset management mount 与 lifecycle skill projection metadata。
- `crates/agentdash-application-vfs/src/provider_skill_asset.rs` - `skill_asset_fs` mount provider，从 Project SkillAsset repository 投影文件。
- `crates/agentdash-application/src/context/builder.rs` - session context bundle 纯 reducer。
- `crates/agentdash-application/src/story/context_builder.rs` - Story owner context contributor。
- `crates/agentdash-application/src/task/context_builder.rs` - Task session context 只读 projection。
- `crates/agentdash-application/src/frame_construction/mod.rs` - FrameLaunchEnvelope 从 pending AgentFrame 构建。
- `crates/agentdash-application/src/frame_construction/assembly.rs` - FrameAssemblyBuilder 投影到 frame surface draft 与 launch extras。
- `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs` - owner bootstrap VFS/capability/MCP/context/memory 组装。
- `crates/agentdash-application-runtime-session/src/session/launch/planner.rs` - launch planner 合并 hook snapshot contribution。
- `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs` - assignment ContextFrame 使用 merged context bundle。
- `crates/agentdash-application-runtime-session/src/session/launch/commit.rs` - accepted launch commit 传递 pending frame。
- `crates/agentdash-application-agentrun/src/agent_run/frame/builder.rs` - AgentFrame context bundle summary 写入 `context_slice_json`。
- `crates/agentdash-application-agentrun/src/agent_run/frame/launch_commit.rs` - pending AgentFrame commit 仅覆盖 capability/VFS/MCP surface。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs` - runtime skill baseline、dynamic provider discovery、live VFS skill merge。
- `crates/agentdash-application-skill/src/skill/loader.rs` - 内建 VFS skill discovery。
- `crates/agentdash-application-skill/src/discovery.rs` - dynamic VFS skill discovery。
- `crates/agentdash-application/src/capability/resolver.rs` - capability directive 到 MCP preset runtime server 的解析。
- `crates/agentdash-application/src/mcp_preset/runtime.rs` - MCP preset runtime binding 应用。
- `crates/agentdash-application/src/vfs_surface_resolver.rs` - ProjectSkillAssets 管理面 VFS。

### Code Patterns

- Shared Library 与 Project SkillAsset 分层总体收束良好。remote import 先 `prepare_remote_skill_import` materialize 并 upsert `LibraryAsset`，再由 facade 调 `install_library_asset_to_project` 安装到 Project SkillAsset；运行 VFS provider 读取的是 Project `SkillAssetRepository.get_by_project_and_key`，不是 LibraryAsset payload。证据：`crates/agentdash-application-skill/src/skill_asset/service.rs:380`, `crates/agentdash-application-skill/src/skill_asset/service.rs:399`, `crates/agentdash-application/src/skill_asset/mod.rs:22`, `crates/agentdash-application/src/skill_asset/mod.rs:30`, `crates/agentdash-application-vfs/src/provider_skill_asset.rs:98`, `crates/agentdash-application-vfs/src/provider_skill_asset.rs:105`。
- Context construction 的核心 reducer 没有 domain 依赖，领域 contributor 只产 `Contribution`，由 `build_session_context_bundle` 统一 upsert/sort。证据：`crates/agentdash-application/src/context/builder.rs:1`, `crates/agentdash-application/src/context/builder.rs:3`, `crates/agentdash-application/src/context/builder.rs:92`, `crates/agentdash-application/src/context/builder.rs:103`, `crates/agentdash-application/src/context/builder.rs:120`。
- Story context 是 owner contributor，不包含 runtime 画像；Task session context 明确是只读视图构建器，不负责 session 启动。证据：`crates/agentdash-application/src/story/context_builder.rs:19`, `crates/agentdash-application/src/story/context_builder.rs:22`, `crates/agentdash-application/src/task/context_builder.rs:39`, `crates/agentdash-application/src/task/context_builder.rs:43`。
- MCP presets 已经主要归口到 CapabilityResolver，并用 frame construction final VFS 作为 runtime binding context。证据：`crates/agentdash-application/src/capability/resolver.rs:144`, `crates/agentdash-application/src/capability/resolver.rs:305`, `crates/agentdash-application/src/capability/resolver.rs:308`, `crates/agentdash-application/src/capability/resolver.rs:310`, `crates/agentdash-application/src/mcp_preset/runtime.rs:80`, `crates/agentdash-application/src/mcp_preset/runtime.rs:85`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:528`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:534`。
- Request-level MCP servers 是显式 runtime override surface，合并后按 server name 去重，并给 request-level MCP 注入 capability。证据：`crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:893`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:897`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:902`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:904`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:910`。
- Lifecycle skill projection 和 ProjectSkillAssets 管理面都是 Project SkillAsset 的 VFS 投影，但常规 owner bootstrap 只把 explicit agent skill keys 投影进 lifecycle surface；ProjectSkillAssets 管理面是独立 `vfs_surface_resolver` surface。证据：`crates/agentdash-application-vfs/src/mount_skill_asset.rs:14`, `crates/agentdash-application-vfs/src/mount_skill_asset.rs:38`, `crates/agentdash-application-lifecycle/src/lifecycle/surface/surface_projector.rs:525`, `crates/agentdash-application-lifecycle/src/lifecycle/surface/surface_projector.rs:531`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:415`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:423`, `crates/agentdash-application/src/vfs_surface_resolver.rs:140`, `crates/agentdash-application/src/vfs_surface_resolver.rs:149`。

### Issue 1: Hook snapshot contribution 在 AgentFrame context summary 之后合并

- Classification: session frame construction ownership split / context injection audit drift / duplicate facts.
- Priority: P1.
- Evidence:
  - `FrameAssemblyBuilder.to_surface_draft` 在 frame construction 阶段从当时的 `context_bundle` 生成 `FrameContextBundleSummary`，写入 surface draft：`crates/agentdash-application/src/frame_construction/assembly.rs:286`, `crates/agentdash-application/src/frame_construction/assembly.rs:296`。
  - `project_frame_assembly_to_frame` 先把 surface draft 写入 `AgentFrameBuilder.with_surface_draft`，launch extras 只是保留完整 bundle：`crates/agentdash-application/src/frame_construction/assembly.rs:351`, `crates/agentdash-application/src/frame_construction/assembly.rs:352`, `crates/agentdash-application/src/frame_construction/assembly.rs:353`, `crates/agentdash-application/src/frame_construction/assembly.rs:355`。
  - `AgentFrameBuilder.with_surface_draft` 将 draft 的 context summary 写入 `context_slice_json`：`crates/agentdash-application-agentrun/src/agent_run/frame/builder.rs:196`, `crates/agentdash-application-agentrun/src/agent_run/frame/builder.rs:209`, `crates/agentdash-application-agentrun/src/agent_run/frame/builder.rs:210`。
  - pending frame 在 planner 之前已经 build 出来，并塞入 envelope：`crates/agentdash-application/src/frame_construction/mod.rs:237`, `crates/agentdash-application/src/frame_construction/mod.rs:246`, `crates/agentdash-application/src/frame_construction/mod.rs:250`, `crates/agentdash-application/src/frame_construction/mod.rs:258`。
  - launch planner 随后解析 hook runtime，把 `hook_snapshot_contribution` merge 到 `launch_envelope.context_bundle`：`crates/agentdash-application-runtime-session/src/session/launch/planner.rs:59`, `crates/agentdash-application-runtime-session/src/session/launch/planner.rs:116`, `crates/agentdash-application-runtime-session/src/session/launch/planner.rs:134`, `crates/agentdash-application-runtime-session/src/session/launch/planner.rs:139`, `crates/agentdash-application-runtime-session/src/session/launch/planner.rs:142`, `crates/agentdash-application-runtime-session/src/session/launch/planner.rs:237`。
  - preparation 用 merge 后的 `context_bundle.bootstrap_fragments` 组装模型可见 assignment context frame：`crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:151`, `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:153`, `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:156`。
  - accepted launch commit 传回的是原 pending frame；AgentRun commit 只用 accepted capability 覆盖 capability/VFS/MCP surface，不重写 context slice：`crates/agentdash-application-runtime-session/src/session/launch/commit.rs:108`, `crates/agentdash-application-runtime-session/src/session/launch/commit.rs:114`, `crates/agentdash-application-agentrun/src/agent_run/frame/launch_commit.rs:126`, `crates/agentdash-application-agentrun/src/agent_run/frame/launch_commit.rs:130`, `crates/agentdash-application-agentrun/src/agent_run/frame/launch_commit.rs:131`。
- Impact:
  - `FrameLaunchEnvelope.context_bundle` / accepted assignment context frame 与 `AgentFrame.context_slice_json` 可能描述不同上下文。
  - 后续 inspector、resume、context usage 或任何以 AgentFrame summary 为准的审计面会漏掉 hook snapshot 注入的 fragment。
  - 如果 hook snapshot 带 workflow constraint、policy、identity/context notice，会形成“模型实际可见，但 frame surface 不承认”的 session frame construction 事实分叉。
- Boundary:
  - Hook snapshot contribution 应进入 frame construction owner bootstrap/request assembler 之前，或 planner 合并后必须通过 AgentFrame frame surface command/builder 重新生成 context bundle summary，并保证 pending frame、launch envelope、audit bundle 使用同一个 bundle。
  - 预研期不需要兼容旧 summary；应保留一个 launch-ready final context fact source。
- 06-14 baseline:
  - 06-14 指出 `SessionRuntimeInner` / hook runtime / context transform 边界过宽，AgentRun/session runtime fact 容易分叉。本轮 FrameConstruction 已收敛多数 capability/VFS/MCP surface，但 hook snapshot 仍在 frame summary 之后注入，是同类问题在新 launch pipeline 中的残留。

### Issue 2: 内建 VFS skill discovery 未接收 launch identity

- Classification: capability/knowledge asset projection ownership gap / identity boundary drift.
- Priority: P1.
- Evidence:
  - owner bootstrap 已把 `spec.identity` 传入 skill baseline projection：`crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:548`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:551`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:555`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:558`。
  - `derive_runtime_skill_baseline` 对内建 workspace VFS skill 调 `load_skills_from_vfs(vfs_service, active_vfs)`，函数签名没有 identity 参数：`crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs:96`, `crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs:102`, `crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs:103`。
  - 同一个 baseline 函数对 dynamic VFS-first provider discovery 会把 `input.identity` 传入 scanner：`crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs:128`, `crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs:145`, `crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs:146`。
  - dynamic scanner 的 API 明确接收 `identity: Option<&AuthIdentity>`，并传入 read/list recursion：`crates/agentdash-application-skill/src/discovery.rs:44`, `crates/agentdash-application-skill/src/discovery.rs:48`, `crates/agentdash-application-skill/src/discovery.rs:71`, `crates/agentdash-application-skill/src/discovery.rs:78`, `crates/agentdash-application-skill/src/discovery.rs:101`, `crates/agentdash-application-skill/src/discovery.rs:109`。
  - 内建 loader 的 read/list 都传 `None` identity：`crates/agentdash-application-skill/src/skill/loader.rs:120`, `crates/agentdash-application-skill/src/skill/loader.rs:195`, `crates/agentdash-application-skill/src/skill/loader.rs:206`, `crates/agentdash-application-skill/src/skill/loader.rs:264`, `crates/agentdash-application-skill/src/skill/loader.rs:295`, `crates/agentdash-application-skill/src/skill/loader.rs:304`。
  - 内建 loader 默认扫描 `lifecycle_vfs`、`canvas_fs`、`skill_asset_fs` 等多个 agent-facing provider：`crates/agentdash-application-skill/src/skill/loader.rs:244`, `crates/agentdash-application-skill/src/skill/loader.rs:248`, `crates/agentdash-application-skill/src/skill/loader.rs:249`, `crates/agentdash-application-skill/src/skill/loader.rs:250`。
- Impact:
  - identity-aware VFS provider 在内建 workspace skill 扫描时会被匿名读取；根据 provider 对 `None` 的解释，可能漏投影本应可见的 skill，或以系统/匿名边界读取不该进入当前 identity 的 skill。
  - 同一 active VFS 中，内建 workspace skills 与 dynamic provider skills 使用不同 identity 语义，导致 capability surface 中 `workspace/*` 与 provider-specific skills 的可见性不可用同一规则解释。
  - 对 knowledge asset 来说，Project SkillAsset/lifecycle/canvas 的 context injection 归属被削弱：launch identity 已经传到 baseline input，但最早的内建 projection 丢失了它。
- Boundary:
  - `load_skills_from_vfs` / `discover_builtin_skill_files` / `read_skill_file` / `list_entries_at` 应接收 identity，并由 `derive_runtime_skill_baseline` 传入 `input.identity`。
  - 内建 workspace skill 和 dynamic VFS-first provider 应共享同一种 VFS access identity 语义；provider key 仍可保持 `workspace`。
- 06-14 baseline:
  - 06-14 baseline 对 VFS projection、skill/lifecycle projection 的 raw/typed gap 有明确担忧。本轮已经把 live VFS skill merge 收敛到 provider identity，并校验 VFS-first provider path；但内建 loader 的 identity 参数缺失仍是 VFS-derived knowledge asset projection 的边界漏洞。

### Non-Issues / Current Convergence

- Shared Library 不再作为 runtime skill fact source。remote skill URL import 写 LibraryAsset，再安装成 Project SkillAsset；运行期 `skill_asset_fs` 读取 Project SkillAsset repository。对照 06-14 的 shared-library/payload typed gap，本轮路径已基本符合 Shared Library contract。
- MCP preset 解析没有发现与 context construction 分叉。ProjectAgent MCP preset 通过 capability directive + resolver 解析，runtime binding 从 final VFS 派生；request-level MCP 是显式 override 并去重。`resolve_preset_mcp_server_refs` 仍存在无 runtime context 的 helper，但本轮没有发现 owner bootstrap/launch 生产路径使用它处理带 runtime binding 的 preset。证据：`crates/agentdash-application/src/mcp_preset/runtime.rs:378`, `crates/agentdash-application/src/mcp_preset/runtime.rs:387`。
- Story/session context 未发现独立事实源分叉。Story contributor、Project contributor、task readonly projection 都通过统一 bundle/reducer 或明确只读 projection 进入链路；前端/API/generated contract 本轮只作为 evidence surface，未作为独立模块审查。
- Lifecycle skill projection 与 ProjectSkillAssets 管理面没有在常规 owner bootstrap 中证明为重复注入。`lifecycle_vfs` 可投影 explicit skill assets；`skill_asset_fs` 是 ProjectSkillAssets 管理 surface。内建 loader 确实允许扫描两类 provider，但本轮未找到常规 launch VFS 同时注入 lifecycle projection 与 `skill-assets` management mount 的生产路径。

### External References

- None. 本轮审查只使用仓库内 `.trellis/` 规范、06-14 baseline 与业务代码。

### Related Specs

- `.trellis/spec/cross-layer/architecture.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
- `.trellis/spec/guides/code-reuse-thinking-guide.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本文件按用户显式给出的 active task path 与唯一允许写入路径写入。
- 未修改业务代码，未运行全量测试。
- 未做外部联网查询；未引用外部版本/文档。
- 前端 feature/store/API route/generated contract 只作为链路边界说明，未做独立泛化审查。
- 未证明 ProjectSkillAssets management mount 与 lifecycle skill projection 在常规 launch VFS 中同时存在；因此没有把“skill asset 双重扫描”列为问题，只记录为 not found。
