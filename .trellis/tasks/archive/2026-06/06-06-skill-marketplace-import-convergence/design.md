# Design · Skill Catalog Source 导入闭环

## 边界

本 child 改造的是 Project 级 URL Import 的写入语义，不改变用户入口。入口仍是：

```text
POST /api/projects/{project_id}/skill-assets/import
```

新的内部链路：

```text
URL
  -> RemoteSkillSource.fetch
  -> SkillTemplateMaterializer
  -> LibraryAsset(remote_imported, skill_template)
  -> install_library_asset_to_project
  -> Project SkillAsset + InstalledAssetSource
```

这样做的原因是 URL import 和后续 catalog import 都是外部来源写入，应该共享 Skill 文件校验、LibraryAsset 版本/digest、Project install/source-status，而不是各自维护 Project 级 source 字段。

## Materializer

建议在 application 层抽出 helper，靠近 `skill_asset` 或 `shared_library`：

```rust
pub struct MaterializedSkillTemplate {
    pub key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub version: String,
    pub source_ref: String,
    pub remote_digest: Option<String>,
    pub payload: SkillTemplatePayload,
}
```

输入可以是：

```rust
pub struct RemoteSkillTemplateInput {
    pub source_kind: RemoteSkillKind,
    pub normalized_url: String,
    pub files: Vec<RemoteSkillFile>,
}
```

Materializer 复用现有逻辑：

- `remote_skill_file_to_input`
- `content_from_bytes`
- `parse_skill_metadata`
- `build_files`
- `validate_skill_files`
- `digest_skill_files`

输出 payload 使用 Shared Library typed schema：

```json
{
  "files": [{ "path": "...", "content": "...", "kind": "skill" }],
  "disable_model_invocation": false
}
```

## Source Identity

`source_ref` 必须稳定、可比较且不泄漏过长 URL 到身份字段。建议：

```text
market:skill-url:{source_kind}:{sha256(normalized_url)}
```

`source_kind` 使用 `github` / `clawhub` / `skills_sh`。`normalized_url` 保留在 payload metadata 之外时，后续 UI 如需展示来源 URL，可通过 detail/source metadata 扩展 contract；本 child 只要求身份稳定。

## Application Flow

建议新增 use case：

```rust
pub async fn import_remote_skill_url_to_project(
    repos: &RepositorySet,
    input: ImportRemoteSkillUrlToProjectInput,
    source: &dyn RemoteSkillSource,
) -> Result<SkillAsset, SkillAssetApplicationError>
```

行为：

1. fetch URL。
2. materialize Skill template。
3. 构造 `LibraryAsset(asset_type=SkillTemplate, scope=User, owner_id=current_user)`。
4. `source=remote_imported`，`source_ref=materialized.source_ref`。
5. 如果同 identity 且同 source_ref 存在，更新 LibraryAsset；其它来源占用返回冲突。
6. 调用 `install_library_asset_to_project`，`overwrite=true` 或等价策略由实现固定并测试。
7. 返回 Project SkillAsset。

## Route Behavior

`skill_assets.rs` route 保持 request/response contract：

```json
{ "url": "https://github.com/org/repo/tree/main/skill" }
```

route 只负责鉴权、URL request 透传、调用 application use case 和 response mapping。写入 Shared Library 的细节不进入 route。

## Data Shape

不新增表。导入结果落在现有：

- `library_assets`: `asset_type=skill_template`, `source=remote_imported`, `source_ref=market:skill-url:*`
- `skill_assets`: Project SkillAsset，带 `installed_source`

`skill_assets.source` 不再承载远端 GitHub/ClawHub/skills.sh 身份，原因是外源版本和审计事实已经由 LibraryAsset 与 InstalledAssetSource 表达。

## Errors

| 条件 | HTTP |
| --- | --- |
| URL 格式非法 / unsupported host | 400 |
| 缺少 `SKILL.md` / metadata 非法 / 文件限制 | 400 |
| Project key 冲突且不能 overwrite | 409 |
| LibraryAsset identity 被其它来源占用 | 409 |
| Remote provider internal error | 500 |

## Tests

- materializer 成功输出 `SkillTemplatePayload`。
- GitHub-like URL import 成功创建 LibraryAsset + Project SkillAsset + InstalledAssetSource。
- invalid remote files 映射 400。
- 重复导入同一 URL 幂等更新 LibraryAsset / Project SkillAsset。
- route 或 application 测试确认 Project edit 权限仍在 route 层校验。
