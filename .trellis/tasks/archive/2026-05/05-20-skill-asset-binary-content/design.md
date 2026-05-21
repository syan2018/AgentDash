# Skill asset 二进制资产存储收敛 — Design

## Overview

`skill_asset` 与 `inline_fs` 都需要表达“归属对象 + 容器 + 路径 + 内容 + metadata”的文件。二者真正的差异不在存储形态，而在上层业务：

- `inline_fs` 是 VFS provider 对 `InlineFile` 的暴露方式。
- `skill_asset` 是 Skill 聚合对同一类嵌入文件的业务视图：skill key、`SKILL.md` metadata、file kind、远端导入来源和 VFS projection。

因此本任务把 `InlineFile` 从 “inline_fs 专属文件” 推成通用 embedded file storage，并让 Skill asset 文件由 `InlineFile(owner_kind=skill_asset, owner_id=<skill_asset_id>, container_id="files")` 承载。`SkillAssetFile` 保留为 domain projection / value object，用于表达 Skill 业务语义，而不是拥有另一套内容存储。

## Shared Content Model

在 `agentdash-domain` 增加共享文件内容类型，建议位置为 `common/file_content.rs`：

```rust
pub enum StoredFileContent {
    Text { content: String },
    Binary { bytes: Vec<u8>, mime_type: String },
}

pub enum StoredFileContentKind {
    Text,
    Binary,
}
```

共享类型负责：

- `kind()`
- `kind.as_str()`
- `text_content()`
- `binary_content()`
- `mime_type()`
- `size_bytes()`
- `into_text()`

`InlineFileContent` 可以被移除或改成 re-export/type alias，`InlineFile` 直接使用 `StoredFileContent`。

## InlineFile As Skill Storage

新增 owner kind：

```rust
pub enum InlineFileOwnerKind {
    Project,
    Story,
    LifecycleRun,
    ProjectAgent,
    SkillAsset,
}
```

Skill 文件存储约定：

- `owner_kind = "skill_asset"`
- `owner_id = skill_assets.id`
- `container_id = "files"`
- `path = Skill 根目录内相对路径，例如 "SKILL.md" / "assets/logo.png"`

这样 Skill 文件内容复用 `inline_fs_files` 的 text/binary schema、blob columns、size/mime metadata 和基础仓储能力。

## SkillAsset Domain Changes

`SkillAssetFile` 作为由 `InlineFile` 映射出的业务视图：

```rust
pub struct SkillAssetFile {
    pub id: Uuid,
    pub skill_asset_id: Uuid,
    pub path: String,
    pub content: StoredFileContent,
    pub kind: SkillAssetFileKind,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

构造方法：

- `new_text(...)`
- `new_binary(...)`
- `new(...)` 保留为 text shortcut，便于现有文本调用点过渡到正确语义。
- `from_inline_file(...)` / `into_inline_file(...)` 负责 Skill 视图和通用存储之间的转换。

业务约束：

- `SKILL.md` 必须是 text。
- `parse_skill_metadata` 只接受 `SKILL.md` 的 text content。
- `SkillAssetFileKind::from_path` 保持业务推断：`SKILL.md` / `scripts/` / `assets/` / reference。

## Persistence

新增 migration `0046_skill_asset_files_to_inline_files.sql`，将旧 `skill_asset_files` 迁移到 `inline_fs_files`：

```sql
INSERT INTO inline_fs_files (
    id, owner_kind, owner_id, container_id, path,
    content_kind, mime_type, text_content, binary_content, size_bytes, updated_at
)
SELECT
    id, 'skill_asset', skill_asset_id, 'files', path,
    'text', NULL, content, NULL, octet_length(content::bytea), updated_at
FROM skill_asset_files
ON CONFLICT (owner_kind, owner_id, container_id, path) DO UPDATE SET ...

DROP TABLE IF EXISTS skill_asset_files;
```

`PostgresSkillAssetRepository::initialize()` 以 `inline_fs_files` 作为 Skill 文件内容来源。它在读写 Skill 聚合时查询/写入 `owner_kind='skill_asset' AND container_id='files'` 的文件行。

事务边界仍由 `PostgresSkillAssetRepository` 负责：创建/更新 `skill_assets` 与替换对应 inline file rows 必须在同一事务中完成。实现时可复用 `InlineFile` row mapping 逻辑，或先在 repository 内部以私有 helper 封装，后续再抽成 infra 共享 helper。

## Application Service

输入类型改为共享内容：

- `SkillAssetFileInput { path, content: StoredFileContent }`
- `RawSkillUploadFile { path, content: StoredFileContent }`

为了保持文本调用点可读，可提供 constructors/helper：

- `SkillAssetFileInput::text(path, content)`
- `SkillAssetFileInput::binary(path, bytes, mime_type)`

上传/导入规则：

- multipart 非 zip：读取 bytes，按 MIME/path 判断 text vs binary。
- zip：读取 entry bytes，不再 `read_to_string`。
- `SKILL.md` 必须 UTF-8 text；不满足则返回 “SKILL.md 必须是 UTF-8 文本”。
- 图片路径或 MIME 为 `image/*` 时保存 binary。
- 其他文本类文件保存 text；未知非 UTF-8 文件作为 binary asset 保存，前提是路径不为 `SKILL.md`。
- 远端导入当前 GitHub/Clawhub/SkillsSh 主要面向 text skill 包；本任务至少保证 GitHub raw bytes 获取后可保存图片。若目录枚举不能发现图片资源，可保留为后续导入增强，但不能再在已获取图片文件时强制 UTF-8。

Digest：

- digest 使用 path + content kind + MIME + raw bytes/text bytes，避免 text/binary 同内容字节产生歧义。

## VFS Projection

`skill_asset_fs` 使用 typed projected file record：

```rust
struct ProjectedSkillAssetFile {
    path: String,
    content: StoredFileContent,
    size_bytes: u64,
    kind: SkillAssetFileKind,
}
```

行为：

- `list`/`stat`: 暴露 `attributes.content_kind`、`attributes.mime_type`、`attributes.skill_asset_file_kind`，并设置 `size`。
- `read_text`: text 返回原内容，binary 返回 `MountError::NotSupported`。
- `search_text`: 跳过 binary。
- `write_text`: 仍只写 text；修改 binary 资源后续通过上传/API 处理。
- `delete_text`/`rename_text`: 对 binary 也应可删除/重命名，方法名虽然是 text primitive，但语义是文件 primitive。`SKILL.md` 限制保持。

## HTTP / DTO

常规 Skill asset DTO：

```rust
pub struct SkillAssetFileDto {
    pub path: String,
    pub content: Option<String>,
    pub content_kind: String,
    pub mime_type: Option<String>,
    pub size_bytes: u64,
    pub kind: Option<String>,
}
```

规则：

- text 文件返回 `content: Some(...)`。
- binary 文件返回 `content: None`，不在 JSON 里内联 bytes。
- create JSON route 只接受 text 文件；update route 可通过 existing binary metadata 保留既有 binary 文件。新 binary 文件通过 upload route 或未来专用 blob route 写入。

新增 Skill asset blob read route：

```text
GET /api/projects/:project_id/skill-assets/:id/files/blob?path=assets/logo.png
```

返回：

- text 文件：400。
- binary image：bytes + `Content-Type`。
- 非 image binary：允许下载或返回 400 可在实现时按 UI 需要定。MVP 建议允许返回 bytes，前端自行决定能否预览。

## Frontend

Skill asset 管理 UI 当前以 text draft 为核心。MVP 只做不破坏：

- 类型和 mapper 支持 `content?: string | null`、`content_kind`、`mime_type`、`size_bytes`。
- `draftFromSkillAsset` 只把 text extra file 放入编辑 draft；binary extra files 不进入文本编辑器。
- 上传入口已有 `uploadSkillAssets(projectId, files)`，可上传目录/zip；上传包含图片后，列表/详情不因 `content` 缺失崩溃。

可选增强：

- 在 Skill asset 详情里展示 binary file metadata 或图片预览。

## Validation

后端重点测试：

- Service：上传分组接受 root skill + image asset。
- Service：`SKILL.md` binary/非 UTF-8 被拒绝。
- Provider：list/stat 暴露 binary metadata；read_text binary rejected；search skips binary。
- Repository：row mapping text/binary。
- API：zip with image upload 不再 UTF-8 失败；blob read 返回 bytes。

前端重点测试：

- mapper 接受 binary file DTO。
- draft conversion 忽略 binary extra files，不把 `undefined` content 写入编辑器。
