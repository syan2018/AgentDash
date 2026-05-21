# Skill asset 二进制资产存储收敛 — Implement Plan

## Phase 0: Planning

- [x] 确认问题：Skill 上传图片失败来自 text-only `SkillAssetFile` / `skill_asset_files.content` / UTF-8 upload extraction。
- [x] 确认原则：Skill asset 是 `inline_fs` 内容模型的业务超集，不应复制二进制存储逻辑。
- [x] 写入 PRD。
- [x] 写入 design.md。

## Phase 1: Shared Domain Content And Inline Storage

- [x] 新增 `agentdash-domain::common::file_content`，定义 `StoredFileContent` / `StoredFileContentKind`。
- [x] 将 `InlineFile` 从 `InlineFileContent` 迁到共享 `StoredFileContent`，保留必要 re-export 以减少调用点 churn。
- [x] 给 `InlineFileOwnerKind` 增加 `SkillAsset`。
- [x] 将 `SkillAssetFile` 从 `String content` 迁到 `StoredFileContent`，增加 `size_bytes` 与 text/binary constructors。
- [x] 增加 `SkillAssetFile` 与 `InlineFile(owner_kind=skill_asset, container_id="files")` 的转换 helper。
- [x] 更新 domain exports。

## Phase 2: Skill Asset Persistence

- [x] 新增 migration `0046_skill_asset_files_to_inline_files.sql`，迁移旧 `skill_asset_files` 到 `inline_fs_files`。
- [x] `PostgresSkillAssetRepository::initialize()` 停止创建 `skill_asset_files`。
- [x] `PostgresSkillAssetRepository` 读写 `inline_fs_files` 的 `owner_kind=skill_asset/container_id=files` 行。
- [x] 保持 `skill_assets` 与对应 inline file rows 的 create/update 事务原子性。
- [x] 删除或停止依赖 `skill_asset_files` row struct / `FILE_COLS`。
- [x] 更新 digest 计算，纳入 kind/mime/raw bytes。

## Phase 3: Application Service Upload / Import

- [x] `SkillAssetFileInput` / `RawSkillUploadFile` 改为 typed content。
- [x] `build_files`、`validate_skill_files`、`parse_skill_metadata` 调整为只读取 `SKILL.md` text。
- [x] multipart upload 非 zip 读取 bytes，不再强制 UTF-8。
- [x] zip extraction 读取 entry bytes，不再 `read_to_string`。
- [x] remote raw fetch 支持 bytes；text-only metadata 文件仍做 UTF-8 校验。
- [x] 更新 service 单元测试，覆盖 image asset 上传分组和 binary `SKILL.md` 拒绝。

## Phase 4: VFS Provider

- [x] `skill_asset_fs` projection 改成 typed projected file。
- [x] list/stat 暴露 `content_kind`、`mime_type`、`size`、`skill_asset_file_kind`。
- [x] `read_text` 对 binary 返回 `NotSupported`。
- [x] `search_text` 跳过 binary。
- [x] write/delete/rename 流程保持 Skill 业务约束并兼容 binary 文件删除/重命名。
- [x] 更新 provider tests。

## Phase 5: API / DTO / Frontend Types

- [x] Skill asset file DTO 增加 `content_kind`、`mime_type`、`size_bytes`，binary 文件不返回 `content`。
- [x] create/update JSON DTO 只接受 text content；update 可保留既有 binary metadata。
- [x] 新增 Skill asset blob read endpoint。
- [x] 更新 `packages/app-web` Skill asset types、mappers、draft conversion。
- [x] 前端补 mapper/draft 单测，并跑 typecheck/lint。

## Phase 6: Validation

- [x] `cargo fmt`
- [x] `cargo check -p agentdash-api`
- [x] `cargo test -p agentdash-application skill_asset`
- [x] `cargo test -p agentdash-application provider_skill_asset`
- [x] `cargo test -p agentdash-api skill_assets`
- [x] `pnpm --filter app-web run typecheck`
- [x] `pnpm --filter app-web run lint`
- [x] `pnpm --filter app-web run test -- skillAsset`
- [x] `git diff --check`

## Risky Files

- `crates/agentdash-domain/src/common/*`
- `crates/agentdash-domain/src/inline_file/entity.rs`
- `crates/agentdash-domain/src/skill_asset/entity.rs`
- `crates/agentdash-application/src/skill_asset/service.rs`
- `crates/agentdash-application/src/vfs/provider_skill_asset.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/skill_asset_repository.rs`
- `crates/agentdash-api/src/dto/skill_asset.rs`
- `crates/agentdash-api/src/routes/skill_assets.rs`
- `packages/app-web/src/services/skillAsset.ts`
- `packages/app-web/src/types/skill-asset.ts`

## Rollback Points

- Shared domain type 前：可回到 Skill-only 二进制支持，但会产生重复模型。
- Migration 前：无 DB 风险。
- Persistence 后：必须保持 domain/service/repo 同步，不能混用旧 `skill_asset_files` 表。
- API DTO 后：前端 mapper 必须同步，否则 binary file 的 `content: null` 会进入文本编辑器。
