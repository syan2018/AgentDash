# inline_fs 图片资产支持 — 技术设计

## Architecture

本任务将 `inline_fs` 从 text-only persistence 升级为 typed inline file persistence。核心边界如下：

- Domain / Infrastructure：负责持久化 text / binary 内容、MIME 类型和 size。
- Application VFS：继续维护文本工具语义，并给 binary 内容提供可列出、可 stat、可读取 blob 的能力。
- API：Surface 文本接口保持文本职责；新增 binary 读写 endpoint 或明确的 blob endpoint。
- Frontend：VFS Browser 根据 metadata 选择文本编辑器或图片预览器。

## Data Model

推荐 schema：

```sql
content_kind   TEXT NOT NULL, -- 'text' | 'binary'
mime_type      TEXT,
text_content   TEXT,
binary_content BYTEA,
size_bytes     BIGINT NOT NULL,
updated_at     TEXT NOT NULL
```

约束：

- `content_kind = 'text'` 时 `text_content IS NOT NULL`，`binary_content IS NULL`。
- `content_kind = 'binary'` 时 `binary_content IS NOT NULL`，`text_content IS NULL`。
- `mime_type` 对 binary 必填；text 可为空或使用 `text/plain` / `text/markdown`。
- `size_bytes` 使用 UTF-8 bytes 长度或 binary bytes 长度，不再使用 Rust `String::len()` 的隐含语义作为唯一来源。

迁移策略：

- 新增 `0045_inline_fs_binary_content.sql`。
- 将旧 `content` 迁移到 `text_content`，`content_kind='text'`，`size_bytes=octet_length(content)`。
- 删除旧 `content` 列，或至少让 repository 主线完全不再读写旧列。预研期推荐删除旧列以保持 schema 正确。
- 更新 `PostgresInlineFileRepository::initialize()` 的 `CREATE TABLE IF NOT EXISTS`。

## Domain Contract

推荐新增：

```rust
pub enum InlineFileContent {
    Text { content: String },
    Binary { bytes: Vec<u8>, mime_type: String },
}
```

`InlineFile` 持有：

- `content: InlineFileContent`
- `mime_type: Option<String>` 或通过 enum 分支派生
- `size_bytes: u64`

Repository trait 推荐新增 binary 专用方法，而不是把所有调用点都改成泛型：

- `get_file(...) -> Option<InlineFile>`
- `list_files(...) -> Vec<InlineFile>`
- `upsert_file(&InlineFile)`
- 保留删除、统计方法

文本写入调用方通过 `InlineFile::new_text(...)`，图片上传通过 `InlineFile::new_binary(...)`。

## VFS Contract

文本接口保持严格文本语义：

- `read_text`：
  - text 文件返回 `ReadResult.content`
  - binary 文件返回 `MountError::NotSupported("binary file cannot be read as text")`
- `write_text`：
  - 写入 text 文件
  - 不负责 binary
- `search_text`：
  - 只搜索 text 文件
  - binary 文件跳过
- `apply_patch`：
  - 只作用于 text 文件

列表和 stat：

- `RuntimeFileEntry.size = Some(size_bytes)`
- `attributes.content_kind = "text" | "binary"`
- `attributes.mime_type = ...`

如需 SPI 级正式 binary 能力，优先增加并行类型：

```rust
pub struct ReadBinaryResult {
    pub path: String,
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub attributes: Option<Map<String, Value>>,
}
```

但 MVP 可以先把 binary 通道放在 `inline_fs` repository + Surface API 层，不强行让所有 provider 实现 binary SPI。

## API Contract

现有 endpoint：

- `read_surface_file`
- `write_surface_file`
- `create_surface_file`
- `list_surface_mount_entries`
- `stat_surface_file`

推荐调整：

- list/stat entry 增加 `content_kind?: string`、`mime_type?: string`。
- text read response 增加 `content_kind: "text"`，如果请求 binary 返回 400/415 风格错误。
- 新增 binary endpoint：
  - `POST /api/vfs/surfaces/files/upload`：multipart 上传到指定 `surface_ref` / `mount_id` / `path`
  - `POST /api/vfs/surfaces/files/blob` 或 `GET .../blob`：读取 binary，返回 bytes 与 `Content-Type`

如果为了统一前端 `api` 客户端，也可以返回 base64 JSON，但仅作为 HTTP DTO，不落库为 base64 文本；推荐 blob response，避免大 JSON。

权限：

- 上传 / 删除 / 重命名使用 `ProjectPermission::Edit`。
- 预览 / 下载使用 `ProjectPermission::View`。
- 仍通过 `ensure_mount_can_write` / `ensure_mount_can_edit` 校验。

## Future Agent Artifact Flow

MVP 不扩展 Agent 工具的二进制写入，但 schema 和 API 应为后续能力保留清晰落点：

1. 图像生成 Agent 或 subagent 在 session 内产生图片 artifact。
2. Runtime 默认将 artifact 登记到会话临时资产区，结构化记录 `artifact_id`、`mime_type`、`size_bytes`、来源 tool call / agent / turn、临时 blob 存储位置。
3. 主 Agent 或用户再选择将 artifact promote 到目标资产位置，例如 `inline_fs://brief/assets/generated/banner.png`。
4. 平台通过受控 API 执行复制、重命名、覆盖策略和 provenance 写入，而不是要求 Agent 直接把 base64 写入文本文件。

这个方向更接近 “artifact promotion / asset management tool”，不是本任务内的 Agent binary write。当前任务只需确保 `inline_fs` 的目标端能保存和展示这类 promoted image asset。长期规划收纳在 Trellis task `05-20-session-artifact-asset-ops`。

### Minimal Bash Consideration

如果未来要让 Agent 主动维护资产，不建议一开始提供通用 bash 作为唯一能力入口。更稳的分层是：

- 第一层：受控 VFS/asset tools，例如 `assets.promote_artifact`、`assets.copy`、`assets.move`、`assets.delete`、`assets.list`。
- 第二层：受限 job runner，用于批量转换、压缩、重命名等可审计任务。
- 第三层：真正 shell / bash，仅在绑定 workspace 或沙盒目录内执行，且不能直接绕过业务权限写 DB。

原因是 `inline_fs` 属于云端业务数据，不是本机文件系统路径；通用 bash 天然擅长操作 OS 文件，但不擅长表达 Project / Story / Permission / provenance / VFS mount 这些平台语义。

## Frontend Design

VFS Browser 状态从 `fileContent: string | null` 扩展为 discriminated union：

```ts
type SelectedVfsFile =
  | { kind: "text"; path: string; content: string; mime_type?: string | null }
  | { kind: "image"; path: string; object_url: string; mime_type: string; size?: number | null }
  | { kind: "binary"; path: string; mime_type?: string | null; size?: number | null };
```

文件选择流程：

1. 从 list/stat metadata 判断是否 image。
2. text 文件调用现有 `readSurfaceFile`。
3. image 文件调用 blob endpoint，创建 object URL，右侧显示图片预览。
4. 切换文件或 unmount 时释放 object URL。

上传流程：

- 仅在 inline_fs 且 mount 支持 create/write 时显示上传按钮。
- 使用 `<input type="file" accept="image/*">`。
- 目标路径默认 `assets/<filename>`，冲突由后端 create/write 语义决定；推荐上传时要求明确 path，并允许覆盖只走 write。
- 上传成功后刷新树并选中新图片。

UI 约束：

- 不把图片放进 CodeMirror。
- 图片预览区域使用稳定尺寸和 object-fit，避免布局跳动。
- 文件树可根据 `mime_type` 展示图片图标，但不是验收必需。

## Tradeoffs

- Base64 text 存储最省改动，但会污染文本工具链、膨胀体积、让 search/apply_patch 和 editor 行为变差；不采用。
- 全 provider binary SPI 架构最完整，但会扩大 relay_fs、skill_asset_fs、canvas_fs 等 provider 的改造范围；MVP 可先局限于 inline_fs + Surface API。
- BYTEA 适合轻量上下文图片资产；大对象存储不是当前目标。

## Operational Notes

- Rust 后端不能热重载；实现后使用 `pnpm dev` 调试时，Rust 变更后需要杀掉旧进程再启动。
- 本任务涉及 migration，调试库需要确认迁移顺序与现有 `initialize()` 一致。
- 若落地时发现图片资产需要被 Agent 模型直接消费，应另起任务设计多模态 context projection，而不是把 base64 注入文本上下文。
