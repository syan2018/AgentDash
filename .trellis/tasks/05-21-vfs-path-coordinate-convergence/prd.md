# VFS 路径与 Inline Mount 写入坐标收敛

## Goal

把当前 VFS Browser 触发的 `mount 'test' 是内联容器，需要 InlineContentOverlay 才能写入` 作为切入点，完整收束 Project VFS Mount、Project / Story inline context、Agent Knowledge、Surface API、Runtime Tool 中散落的 inline mount 写入路径与地址坐标，让 VFS 对外只剩一套稳定模型：

```text
surface_ref + mount_id + mount_relative_path
```

内部存储坐标（owner_kind / owner_id / container_id / root_ref / project_vfs_mount.id）只由后端 resolver 生成和消费，上层 API、前端、Agent 工具与 route handler 不再各自拼接或分支判断。

## User Value

- 用户在 VFS Browser 中浏览、创建、保存、删除、重命名 inline 文件时，不再因为不同操作走了不同写入路径而触发内部实现错误。
- 开发者排查 VFS 行为时只需要跟随一条分发链路：Surface/Tool 输入 → Runtime mount → Provider/Writer，而不是在 route、service、provider、repo 之间反复确认坐标含义。
- Project VFS Mount 扁平化后的模型继续保持清晰：`mount_id` 是外部唯一标识，数据库 UUID 只服务内部持久化，inline 文件落库坐标不泄漏到 UI/API。
- 后续新增 provider 或文件操作时，必须接入统一写入端口，减少 create/write/delete/rename/apply_patch 之间的行为漂移。

## Confirmed Facts

- 当前报错由 `VfsBrowserPanel.handleCreateFile` 调用 `createSurfaceFile` 触发；前端传入的是 `surface_ref + mount_id + path`。
- `crates/agentdash-api/src/routes/vfs_surfaces.rs::create_surface_file` 当前通过 `vfs_service.create_text(..., None, None)` 创建文件，缺少 inline overlay。
- `RelayVfsService::create_text` 内部会落到 `write_text`；`write_text` 对 `inline_fs` mount 要求传入 `InlineContentOverlay`，否则返回该报错。
- 同一 route 文件里，`write_surface_file` 已经单独创建 `DbInlineContentPersister + InlineContentOverlay`；`delete_surface_file` / `rename_surface_file` 又直接操作 `inline_file_repo`；`apply_surface_patch` 走 overlay。这说明写操作路径存在分叉。
- `ProjectVfsMount` 已经是 Project 级 VFS 的一等公民，API 外部标识为 `mount_id`，inline 文件内部存储为 `inline_fs_files(owner_kind="project_vfs_mount", owner_id=mount.id, container_id="files")`。
- Story / Project config 的 `ContextContainerDefinition::InlineFiles` 与 Project Agent Knowledge 同样生成 `inline_fs` runtime mount；它们共享同一套 inline storage resolver 与 mutation 通道，业务配置模型暂不改变。
- `build_project_vfs_mount_mount` 目前同时设置 `root_ref = project-vfs-mount://{uuid}`、`metadata.container_id = "files"`、`metadata.agentdash_context_container_id = mount_id`。这两个 container 概念同时出现，语义容易混淆。
- `.trellis/spec/backend/vfs/vfs-access.md` 已定义核心契约：所有资源访问统一为 `mount + relative path`，原始字符串只能存在于 UI/API/relay/tool 输入边界，进入 application 内部前必须 parse/normalize 成结构化地址。
- 项目处于预研期，可以 hard cut；本项目不需要为旧 API 或旧数据库字段保留兼容分支，但 schema/migration 需要保持正确。

## Requirements

### R1 — 唯一外部地址模型

- VFS Surface API、VfsBrowser、Agent fs 工具、运行时 preview 一律只接收并传递：
  - `surface_ref`
  - `mount_id`
  - `mount_relative_path`
- Project VFS Mount 的数据库 UUID、inline owner 坐标、`container_id="files"` 只存在于后端 resolved runtime mount 或内部 writer 中。
- `root_ref` 作为 provider root identity，不作为 UI/API 可编辑路径，也不参与用户输入路径拼接。
- Project / Story context inline containers 与 Agent Knowledge 对外同样只暴露 mount identity，不暴露 inline storage key。

### R2 — Inline 写入统一端口

- inline text 写操作统一经过一个 application 层端口，例如 `InlineMountWriter` / `VfsMutationDispatcher` / `ResolvedMountWriter`。
- Surface route 不再分别手写：
  - create 走 `vfs_service.create_text(... None ...)`
  - write 走本地 overlay
  - delete / rename 直接 repo
  - apply_patch 走 overlay
- create / write / delete / rename / apply_patch 对 inline_fs 的 existence check、路径 normalize、事件语义、持久化语义保持一致。
- blob upload 作为 binary 写入入口也要接入同一套 inline owner resolver，避免单独解析 owner 坐标。
- Project VFS Mount、Project config inline container、Story inline container、Project Agent Knowledge inline mount 都必须由同一 resolver 和 mutation dispatcher 覆盖。

### R3 — Provider 分发边界清晰

- `RelayVfsService` 继续作为 runtime VFS 分发器，但不要求 route handler 理解 inline overlay 细节。
- `InlineFsMountProvider` 负责 inline read/list/search/stat/binary read；写入可以由统一 mutation dispatcher 调用 provider writer 或 inline persister，但不能在多个 route handler 中散落 repo 直写。
- `MountProviderRegistry` 的 edit capabilities 仍是按钮和权限展示来源；Surface mutation 执行时以同一入口再次校验。

### R4 — 坐标类型化与命名收束

- 引入或强化结构化类型：
  - `SurfaceRef`
  - `MountId`
  - `MountRelativePath`
  - `ResolvedMountAddress`
  - `InlineStorageKey`
- `container_id` 命名只表示 inline storage container；Project VFS Mount 的对外 `mount_id` 使用 `mount_id` / `agentdash_context_container_id` 这类字段时要明确用途，避免同一个词在 UI 身份和存储容器之间漂移。
- 同一 runtime mount 中只保留必要 metadata。能由 `ProjectVfsMountContent::Inline` + mount UUID 推导的字段不在多个地方重复表达。

### R5 — Frontend 表达跟随后端收敛

- VfsBrowser 继续只提交 `surfaceRef + mountId + path`，不引入 inline owner / UUID / root_ref 输入。
- Mount summary 中保留帮助用户理解的 display 信息，但内部字段不作为编辑路径展示。
- Inline Project VFS Mount 的文件操作错误应体现用户语义，如“文件已存在”“路径非法”“当前 mount 没有 write 能力”，而不是暴露 `InlineContentOverlay` 这类内部实现名。

### R6 — Spec 与测试闭环

- 更新 VFS spec，把 Surface mutation 与 inline storage resolver 的唯一路径写清楚。
- 补充后端测试覆盖 inline surface create/write/delete/rename/apply_patch/blob upload 的统一行为。
- 补充前端测试或 service-level 测试，确保 VfsBrowser 对 inline mount 的 create/save/delete/rename 调用 API payload 仍是唯一外部模型。

## Acceptance Criteria

- [ ] 在 VFS Browser 的 Inline Project VFS Mount 中新建 `new-file.txt` 成功落库，刷新后仍可见。
- [ ] create / write / delete / rename / apply_patch / blob upload 对 inline_fs 不再出现 `需要 InlineContentOverlay`。
- [ ] Surface mutation route 不再直接分散解析 inline owner 坐标或直接操作 `inline_file_repo`；统一通过 application 层 mutation 端口。
- [ ] Project VFS Mount、Project config inline container、Story context inline mount、Project Agent Knowledge inline mount 的写入坐标都由同一个 resolver 生成。
- [ ] 用户可见 API 与前端 payload 中不出现 `owner_kind` / `owner_id` / internal mount UUID / inline storage `container_id`。
- [ ] `root_ref`、`container_id`、`mount_id` 在 runtime mount metadata 中语义明确；重复字段被删除或通过注释/类型收束为单一用途。
- [ ] 相关错误映射成用户语义化 BadRequest / NotFound / Conflict / ServiceUnavailable，不暴露内部 overlay 实现。
- [ ] `cargo test -p agentdash-application vfs`、`cargo test -p agentdash-api vfs`、`pnpm --filter app-web test -- vfs-browser-panel` 至少覆盖本任务关键路径并通过。
- [ ] `.trellis/spec/backend/vfs/vfs-access.md` 更新 Surface mutation 与 inline storage resolver 契约。

## Scope Boundary

- 本任务聚焦 VFS 地址与 inline mutation 收敛。
- `inline_fs_files` typed text/binary schema 维持现状。
- ProjectVfsMount 单层模型维持现状；本任务只整理其 runtime/storage 坐标表达。
- Project / Story 局部 `context_containers` 仍然保留为业务配置来源，但运行时写入和文件操作必须进入同一 inline mutation 通道。
- Materialization 仍遵循现有公共只读/工作副本契约；本任务不把 materialized path 变成 VFS 写入口。

## Open Questions

无。用户已确认本任务必须完整覆盖所有 `inline_fs` runtime mount：Project VFS Mount、Project config inline container、Story inline container、Project Agent Knowledge。
