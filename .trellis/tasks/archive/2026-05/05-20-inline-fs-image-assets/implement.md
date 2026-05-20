# inline_fs 图片资产支持 — 实施计划

## Phase 0: Planning Review

- [x] 用户确认 MVP 范围：本任务暂不要求 Agent 直接写入图片二进制。
- [x] 用户确认未来 Agent 生成图片默认先进入 session 临时资产区，再由用户或主 Agent promote 到 `inline_fs`。
- [x] 用户确认继续进入 implementation 后，执行 `task.py start`。

## Phase 1: Backend Persistence

- [x] 新增 migration `0045_inline_fs_binary_content.sql`。
- [x] 更新 `PostgresInlineFileRepository::initialize()` 的 table schema。
- [x] 更新 `InlineFile` domain entity，增加 text / binary content 表达。
- [x] 更新 `InlineFileRepository` trait 与调用方构造方法。
- [x] 更新 Postgres repository SELECT / INSERT / UPSERT / row mapping。
- [x] 更新 `sync_container_inline_files`，把 ContextContainer 初始文件写为 text。
- [ ] 补 repository 单元测试或集成测试，覆盖 text 迁移后读写、binary upsert/list/get/delete。（本轮用 provider/API 现有路径覆盖，未接入真实 Postgres 测试。）

## Phase 2: Application VFS

- [x] 更新 `InlineFsMountProvider` 的 list/read/search 行为。
- [x] list/stat entry 填充 `size` 和 `attributes.content_kind` / `attributes.mime_type`。
- [x] binary 文件 `read_text` 返回明确错误。
- [x] text search 跳过 binary 文件。
- [x] `InlineContentOverlay` 保持 text write-through 语义；binary 写入走 Surface API + repository。
- [x] 更新 relay service 中 search/stat 对 binary 的边界处理。

## Phase 3: Surface API

- [x] Surface list/stat/read DTO 增加 `content_kind` / `mime_type` metadata。
- [x] 新增 inline binary upload endpoint，使用 multipart。
- [x] 新增 binary read/download endpoint，返回 blob bytes 和正确 `Content-Type`。
- [x] API 层对 binary 调用 text read/write 返回清晰错误。
- [x] 权限和 mount edit capability 复用现有校验。
- [ ] 补 API tests，覆盖上传、读取、列表 metadata、权限失败、text/binary 边界。（本轮已跑现有 VFS API tests，未新增 multipart route test。）

## Phase 4: Frontend VFS Browser

- [x] 更新 `packages/app-web/src/services/vfs.ts` 类型与 API 方法。
- [x] 更新 `VfsFileTree` entry 类型，保留 snake_case DTO 字段。
- [x] 将 `VfsBrowserPanel` 的 selected file state 扩展为 text / binary image 分支。
- [x] 新增图片上传按钮和 hidden file input。
- [x] 新增图片预览组件，处理 loading/error/object URL revoke。
- [x] 文本文件继续使用 `VfsCodeEditor`，图片文件不进入 CodeMirror。
- [ ] 增加前端测试，覆盖 metadata 分支、上传调用、图片预览切换。（本轮跑现有 VFS browser tests，未新增 React DOM 行为测试。）

## Phase 5: Validation

- [x] `cargo check -p agentdash-api`
- [x] `cargo test -p agentdash-application provider_inline`
- [x] `cargo test -p agentdash-api vfs_access`
- [x] `pnpm --filter app-web run typecheck`
- [x] `pnpm --filter app-web run lint`
- [x] `pnpm --filter app-web run test -- vfs-browser-panel`
- [x] 使用 `pnpm dev` 启动；API `http://127.0.0.1:3001/api/health` 与前端 `http://127.0.0.1:5381/` HTTP 探活通过。
- [ ] 如有 UI 变化，使用浏览器验证桌面宽度下文件树、文本编辑器、图片预览不重叠。（本轮完成 HTTP 探活，未做真实浏览器图片上传流。）

## Risky Files

- `crates/agentdash-domain/src/inline_file/entity.rs`
- `crates/agentdash-domain/src/inline_file/repository.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/inline_file_repository.rs`
- `crates/agentdash-application/src/vfs/provider_inline.rs`
- `crates/agentdash-application/src/vfs/inline_persistence.rs`
- `crates/agentdash-application/src/vfs/relay_service.rs`
- `crates/agentdash-api/src/routes/vfs_surfaces.rs`
- `packages/app-web/src/services/vfs.ts`
- `packages/app-web/src/features/vfs/vfs-browser-panel.tsx`
- `packages/app-web/src/features/vfs/vfs-file-tree.tsx`

## Rollback Points

- Migration 前：无 DB schema 风险。
- Persistence 完成后：可以回滚到 text-only repository，但不能混用新旧 schema 主线。
- API 完成后：binary endpoint 可隐藏在前端入口之外，但 repository schema 应保持新结构。
- Frontend 完成后：可临时隐藏上传按钮，保留后端能力。
