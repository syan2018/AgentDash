# 统一 VFS（跨层契约）

> Agent 和上层用例不直接感知 `backend_id + absolute path`，统一使用 `mount + relative path` 模型。

---

## 核心设计

所有资源访问统一为 `mount + relative path`：

- `mount` 是会话级挂载 ID（如 `main` / `spec` / `brief` / `lifecycle`）
- `path` 是相对 mount 根的路径
- 每个 session 启动时生成一份 mount table

Application 层的最小地址类型包括：

- `MountId`
- `MountRelativePath`
- `VfsUri`
- `RootRef::LocalPath | RootRef::ProviderUri`
- `PathPolicy`

原始字符串只能存在于 UI/API/relay/tool 输入边界；进入 application 内部前必须 parse/normalize 成结构化地址。

## 运行时工具集

稳定的小工具集合：

- `mounts.list` — 列出当前会话可访问的 mount 清单
- `fs.read` / `fs.write` / `fs.apply_patch` — 文件读写
- `fs.list` / `fs.search` — 目录和内容搜索
- `shell.exec` — 命令执行（仅限声明了 `exec` 能力的 mount）

所有工具使用统一参数模型：`{ "mount": "main", "path": "relative/path" }`

## Provider 能力矩阵

| 能力 | 物理 workspace | KM / Snapshot | Lifecycle VFS |
| --- | --- | --- | --- |
| `read` | 必须 | 必须 | 必须 |
| `write` | 可选 | 按 provider | 受限（artifacts/records） |
| `list` | 必须 | 必须 | 必须 |
| `search` | 推荐 | 推荐 | — |
| `exec` | 可选 | 不支持 | 不支持 |
| `watch` | — | — | 可选（通知机制） |

当前已落地的 provider：
- `relay_fs`：通过 relay 访问本机物理工作空间
- `inline_fs`：云端 Project/Story 配置导出的内联只读文件
- `lifecycle_vfs`：将 LifecycleRun 暴露为虚拟文件系统

## 错误矩阵

| 条件 | 错误语义 |
| --- | --- |
| mount 不存在 | `NotFound` |
| path 为绝对路径或含 `..` 越界 | `InvalidPath` / `PathEscapesMount` |
| mount 不支持该能力 | `CapabilityDenied` |
| 目标 backend 不在线 | `BackendOffline` |
| relay 超时 | `Timeout` |

## 关键契约

1. **资源定位**：Agent 不应直接感知 `backend_id` 或绝对路径
2. **一致性**：声明式来源解析与运行时工具访问共享同一套 provider 底座
3. **relay 隔离**：relay 是 transport，不是 mount 模型；上层不直接拼接 `RelayMessage`
4. **写入约束**：`fs.write` 默认只允许 `default_write=true` 的 mount；`fs.apply_patch` 受 `write` 能力约束
5. **物化路径**：VFS URI 转本机 path 必须遵守 [vfs-materialization.md](./vfs-materialization.md)
6. **root_ref 类型化**：`RootRef` 必须区分本机路径和 provider URI；`lifecycle://`、`skill-assets://`、`canvas://` 等虚拟 root 不得隐式转为 OS `PathBuf`
7. **路径硬校验**：`Vfs` 构建/派生后必须执行 hard validation，至少检查 mount id 唯一、系统保留 mount id 未被错误 provider 占用、default mount 存在、root_ref/provider scheme 合法、内置 provider capability 与支持范围一致、link target 存在且无环
8. **预研期约束**：不保留旧路径行为回退；发现非法地址应直接失败并补测试

## 内核扩展能力

### Extended Attributes

Provider 可在 `read` / `list` / `stat` 结果中附带结构化元数据（`attributes` 字段），替代在文件内容开头嵌入 YAML frontmatter 的做法。

## Scenario: inline_fs 图片资产与二进制内容

### 1. Scope / Trigger

- Trigger: `inline_fs` 从 text-only 文件扩展为 text / binary typed inline file storage。
- Scope: `inline_fs_files` DB schema、`InlineFile` domain model、Surface VFS API、前端 VFS Browser。
- Non-goal: Agent `fs.write` 不直接写图片二进制；Agent 生成图片归档另走 session artifact promotion 设计。

### 2. Signatures

DB `inline_fs_files` 内容列：

```sql
content_kind   TEXT NOT NULL, -- 'text' | 'binary'
mime_type      TEXT,
text_content   TEXT,
binary_content BYTEA,
size_bytes     BIGINT NOT NULL
```

Domain content model:

```rust
pub enum InlineFileContent {
    Text { content: String },
    Binary { bytes: Vec<u8>, mime_type: String },
}
```

Surface API additions:

```text
POST /api/vfs-surfaces/read-file-blob
POST /api/vfs-surfaces/upload-file-blob
```

### 3. Contracts

- `content_kind = "text"`: `text_content IS NOT NULL` and `binary_content IS NULL`.
- `content_kind = "binary"`: `binary_content IS NOT NULL`, `text_content IS NULL`, and `mime_type IS NOT NULL`.
- `list` / `stat` for inline files must expose:
  - `size`
  - `attributes.content_kind`
  - `attributes.mime_type`
- Surface list/stat DTO mirrors those fields as `content_kind` / `mime_type` in snake_case.
- Text APIs (`read-file`, `write-file`, `create-file`, `apply-patch`) remain text APIs.
- Blob read API uses `MountProvider::read_binary`; `inline_fs` and `skill_asset_fs` expose image assets through the same provider contract.
- Blob upload API is limited to `inline_fs` mounts.

### 4. Validation & Error Matrix

| Condition | Error |
| --- | --- |
| `read_text` on binary inline file | `MountError::NotSupported` |
| `search_text` sees binary inline file | Skip file |
| `apply_patch` targets binary inline file | Text read fails; do not coerce binary to text |
| Blob read on text file | HTTP 400 |
| Blob read on provider without `read_binary` support | HTTP 400 |
| Blob upload on non-`inline_fs` mount | HTTP 400 |
| Upload MIME is not `image/*` | HTTP 400 |
| Invalid mount-relative path | HTTP 400 via path normalization |

### 5. Representative Cases

- Image asset: Upload `assets/logo.png` as `image/png`; list returns `content_kind=binary`, `mime_type=image/png`, `size=<bytes>`; VFS Browser renders image preview.
- Skill asset image: `skill_asset_fs` image entries use `read_binary`, so VFS Browser renders previews from the generic blob read API.
- Text file: Existing `note.md` remains `content_kind=text`; `fs.read` and CodeMirror editing still work.

### 6. Tests Required

- Provider unit test: binary inline file appears in list/stat metadata.
- Provider unit test: `read_text` rejects binary and `search_text` skips binary.
- API test: multipart image upload writes binary row and blob read returns bytes with `Content-Type`.
- Frontend test: VFS Browser routes image entries to blob preview, not `VfsCodeEditor`.

### 7. Canonical Construction

```rust
let file = InlineFile::new_binary(
    owner_kind,
    owner_id,
    "brief",
    "assets/logo.png",
    png_bytes,
    "image/png",
);
```

### Projection（虚拟文件）

`is_virtual=true` 标记条目是 provider 动态投影的内容（物理存储中不存在），如 `lifecycle_vfs` 的 `active/*`、`nodes/*`、`runs/*` 路径。

### Mount Link

声明式的 mount 级重定向（不是通用 symlink），`parse_mount_uri` 自动跳转。最大跳转深度 5 层。用于 workflow step input 引用上游 output、共享文档引用等场景。

### Watch / 事件通知

Provider 可通过 broadcast channel 推送 `MountEvent`（Created/Modified/Deleted/Renamed），供 Application 层内部消费（Workflow 编排、Hook runtime），暂不在 Agent tool 层暴露。

## Scenario: SkillAsset 文件复用 InlineFile 存储

### 1. Scope / Trigger

- Trigger: Skill asset 文件从 text-only `skill_asset_files.content` 收敛到通用 embedded file storage。
- Scope: `SkillAssetFile` domain projection、`PostgresSkillAssetRepository` 文件读写、`skill_asset_fs` VFS projection、Skill 上传/导入路径。
- Storage owner: `InlineFileOwnerKind::SkillAsset`。

### 2. Signatures

Skill asset 文件内容必须存储在 `inline_fs_files`：

```text
owner_kind   = "skill_asset"
owner_id     = skill_assets.id
container_id = "files"
path         = Skill 根目录内相对路径
```

Skill asset file DTO：

```rust
pub struct SkillAssetFileDto {
    pub path: String,
    pub content: Option<String>,
    pub content_kind: String, // "text" | "binary"
    pub mime_type: Option<String>,
    pub size_bytes: u64,
    pub kind: Option<String>,
}
```

`SkillAssetFile` 是 Skill 领域视图。文件内容生命周期由 `InlineFile` 承担；Skill 视图携带 `kind`、metadata validation 结果和主文档规则等业务语义。

### 3. Contracts

- `SKILL.md` 必须是 UTF-8 text；metadata 解析只读取该文本主文档。
- 图片等二进制资源保存为 `StoredFileContent::Binary`，并保留 `mime_type` / `size_bytes`。
- Skill asset JSON DTO 不内联 binary bytes；binary 文件只返回 `content_kind` / `mime_type` / `size_bytes` metadata。
- 本地目录/ZIP 上传可携带图片资产；前端在发送前限制总大小，后端 upload route 必须显式设置 multipart body limit，避免默认小 body limit 在请求体阶段断连。
- `skill_asset_fs` list/stat 暴露 `content_kind`、`mime_type`、`skill_asset_file_kind`。
- `skill_asset_fs.read_text` 对 binary 返回 `NotSupported`。
- `skill_asset_fs.search_text` 跳过 binary。

### 4. Validation & Error Matrix

| Condition | Error / Behavior |
| --- | --- |
| 上传 `SKILL.md` 非 UTF-8 | BadRequest: `SKILL.md 必须是 UTF-8 文本文档` |
| 上传图片资源 | Store as `StoredFileContent::Binary` |
| DTO 返回 binary file | `content = None`，metadata 保留 |
| JSON create 带 `content = None` | BadRequest |
| JSON update 带 existing binary metadata | Preserve existing binary content |
| 本地上传超过前端总大小限制 | 前端阻止请求并显示明确错误 |
| 本地上传超过后端 multipart body limit | HTTP 413 |
| `skill_asset_fs.read_text` 读取 binary | `MountError::NotSupported` |
| `skill_asset_fs.search_text` 遇到 binary | Skip |

### 5. Representative Cases

- Skill with image: 上传包含 `SKILL.md` 和 `assets/logo.png` 的 Skill；`SKILL.md` 解析 metadata，logo 存入 `inline_fs_files(owner_kind='skill_asset')` 且 DTO 只返回 metadata。
- Text Skill: 文本-only Skill 仍可创建、编辑、通过 `skill_asset_fs` 被 skill loader 发现。

### 6. Tests Required

- Service: 上传分组接受 root `SKILL.md` + binary image asset。
- Service: binary / non-UTF8 `SKILL.md` 被拒绝。
- Provider: binary file list/stat metadata 正确，`read_text` rejected，`search_text` skipped。
- Frontend mapper: binary file DTO 不进入 text draft，但 update payload 能保留 binary metadata。
- Frontend upload guard: oversized local directory upload is rejected before `fetch`.

### 7. Canonical Construction

```rust
InlineFile::new_binary(
    InlineFileOwnerKind::SkillAsset,
    skill_asset_id,
    "files",
    "assets/logo.png",
    png_bytes,
    "image/png",
);
```

### Migration Contract

旧 `skill_asset_files` 行迁移到 `inline_fs_files(owner_kind='skill_asset', container_id='files')` 后，Repository 主线以 `InlineFile` 通用存储作为 Skill 文件内容来源。

## Scenario: Agent fs_read 图片 Block

### 1. Scope / Trigger

- Trigger: Agent runtime tool 需要把已存入 typed VFS 的图片资产作为模型可消费图片输入返回。
- Scope: `MountProvider::read_binary`、`RelayVfsService::read_binary`、`fs_read` tool result、connector image block 映射。
- Storage scope: `read_binary` 是 `Read` capability 下的 typed content channel；不新增独立 mount capability。

### 2. Signatures

Provider binary read:

```rust
pub struct BinaryReadResult {
    pub path: String,
    pub data: Vec<u8>,
    pub mime_type: String,
    pub attributes: Option<serde_json::Map<String, serde_json::Value>>,
}
```

Tool image result:

```rust
ContentPart::Image {
    mime_type: "image/png".to_string(),
    data: base64_bytes,
}
```

`ContentPart::Image.data` 始终是原始 bytes 的 standard base64，不包含 `data:` URL 前缀。需要 URL 的协议边界由 connector / UI adapter 用 `mime_type + data` 组装。

### 3. Contracts

- `fs_read` 先通过 `stat`/metadata 判断 `attributes.content_kind == "binary"`，不按文件扩展名猜测。
- `image/*` binary 返回 text metadata block + image block。
- 非 image binary 返回 `is_error = true` 的文本结果，不把 bytes 写入模型上下文。
- 文本文件继续走 `read_text`，保留行号输出与 `start_line` / `end_line` 语义。
- `inline_fs` 与 `skill_asset_fs` 实现 `read_binary`；其它 provider 默认 `NotSupported`。
- Anthropic image block 消费纯 base64；Codex app protocol `InputImage.image_url` 由边界 adapter 组装为 data URL。

### 4. Validation & Error Matrix

| Condition | Error / Behavior |
| --- | --- |
| `fs_read` 读取 `image/*` binary | `content=[Text, Image]`, `is_error=false` |
| `fs_read` 读取非 image binary | `is_error=true`, text explains unsupported MIME |
| binary entry 缺少 `mime_type` | tool execution error |
| `read_binary` on text file | `MountError::NotSupported` |
| provider 未实现 `read_binary` | `MountError::NotSupported` |

### 5. Representative Cases

- Inline image: `brief://assets/logo.png` stat exposes `content_kind=binary`, `mime_type=image/png`; `fs_read` returns `ContentPart::Image { mime_type="image/png", data="<base64>" }`。
- Skill asset image: `skill-assets://skills/writer/assets/logo.png` follows the same provider contract through `SkillAssetFile` projection。
- Archive file: `brief://assets/bundle.zip` returns unsupported binary text result and no image block。
- Text file: `brief://note.md` still returns numbered text lines.

### 6. Tests Required

- Provider unit: `inline_fs.read_binary` returns bytes, MIME and content metadata.
- Provider unit: `skill_asset_fs.read_binary` returns projected bytes, MIME and skill metadata.
- Tool unit: `fs_read` covers text result, image result, unsupported binary result.
- Connector unit: Anthropic tool result preserves image block as base64 source.
- Protocol mapper unit: Codex app dynamic tool result maps `ContentPart::Image` to data URL.

---

*创建：2026-04-17 — 统一 VFS 跨层契约*
*精简：2026-05-16 — 移除代码复述、测试列表、实施计划；保留核心契约和能力矩阵*
*更新：2026-05-16 — 补充资源地址类型、root_ref 类型化与 VFS hard validation 契约*
*更新：2026-05-20 — 补充 inline_fs text/binary 内容契约与图片资产 API 边界*
*更新：2026-05-20 — SkillAsset 文件收敛到 InlineFile embedded storage*
*更新：2026-05-20 — 补充 Agent fs_read 图片 Block 与 read_binary 契约*
