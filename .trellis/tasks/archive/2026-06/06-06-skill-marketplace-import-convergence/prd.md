# PRD · Skill Catalog Source 导入闭环

## 背景

父任务要求外部来源写入最终收束到 Shared Library / Project Asset 模型。当前项目已有 GitHub / ClawHub / skills.sh 单 URL Skill 导入机制，但它直接通过 `POST /api/projects/{project_id}/skill-assets/import` 创建 Project `SkillAsset(source = github | clawhub | skills_sh)`，没有生成 `skill_template` LibraryAsset，也没有记录 Shared Library `InstalledAssetSource`。

本 child 负责把现有远端 Skill URL 导入改造成外部来源导入链路：URL 仍作为单项远端定位，fetch、文件数量/大小、`SKILL.md` 和 metadata 校验继续复用现有能力；写入改为先创建/更新 `LibraryAsset(source = remote_imported, asset_type = skill_template)`，再调用 Shared Library install 入口生成 Project SkillAsset。

## 用户价值

- GitHub / ClawHub / skills.sh 与后续 catalog import 进入同一条 Shared Library 写入链。
- Project SkillAsset 的外源身份通过 `InstalledAssetSource` 表达，Marketplace source-status 和可更新提示能复用既有模型。
- 现有 URL Import UI 可以保持用户入口不变，但后端写入语义与外部 Marketplace API 一致。

## 确认事实

- `HttpRemoteSkillSource` 已支持 GitHub / ClawHub / skills.sh URL fetch，并在基础设施层限制文件数量、单文件大小和总大小。
- `SkillAssetService::import_remote` 当前把 fetched files 直接转成 Project `SkillAsset`。
- `SkillAssetService::create_from_remote_files_typed` 已包含 `SKILL.md` metadata 解析、content typing、key 冲突检查和 `validate_skill_files`。
- Shared Library install 已能把 `skill_template` LibraryAsset 安装为 Project SkillAsset，并写入 `InstalledAssetSource`。
- External Marketplace API 已提供 `import_external_marketplace_asset` helper，可写入 `LibraryAsset(source=remote_imported)`。

## 目标

R1. 抽出远端 Skill materializer，把 fetched `RemoteSkillFile` / `SkillAssetFileInput` 统一转换成 `SkillTemplatePayload`、metadata、digest 和稳定 key。

R2. `POST /api/projects/{project_id}/skill-assets/import` 保持 API 入参不变，但后端写入改为：

1. 校验 Project edit 权限。
2. 使用 `HttpRemoteSkillSource` fetch URL。
3. materialize 为 `skill_template` LibraryAsset payload。
4. 创建或更新 `LibraryAsset(source=remote_imported)`。
5. 调用 Shared Library install 入口安装到 Project SkillAsset。
6. 返回安装后的 `SkillAssetResponse`。

R3. URL import 生成的 `source_ref` 使用稳定外部来源身份，例如 `market:skill-url:{source_kind}:{normalized_url_hash}` 或等价不会受显示 URL 细节漂移影响的格式。

R4. 远端 URL / source kind / digest 归属到 LibraryAsset `source_ref`、`version`、`payload_digest` 与 Project `InstalledAssetSource`；Project SkillAsset 不再新写 `github | clawhub | skills_sh` source 事实。

R5. 保留现有远端 Skill 校验能力：缺少根目录 `SKILL.md`、非法 metadata、非法文件、文件限制、重复 key 都必须继续失败。

R6. 当前 child 不实现真正 catalog list/search provider，也不改 Marketplace 外部来源 UI；它只打通单项 URL import 与 Shared Library install 链路，为后续 catalog provider 复用 materializer。

## 非目标

- 不实现目录型 Skill catalog list/search/detail UI。
- 不实现 MCP catalog。
- 不新增 marketplace source cache 表。
- 不做旧数据迁移；项目尚未上线，当前迁移只需保持 schema 最正确状态。

## 验收标准

- [ ] URL import 成功后数据库/领域对象中先存在 `skill_template` LibraryAsset，再存在带 `InstalledAssetSource` 的 Project SkillAsset。
- [ ] 返回给前端的 `SkillAssetResponse` 表达 Project SkillAsset，现有 UI 调用无需改入参。
- [ ] GitHub / ClawHub / skills.sh fetch、content typing、`SKILL.md` metadata 和文件限制仍由后端统一校验。
- [ ] 重复导入同一远端 Skill 可更新同一 `remote_imported` LibraryAsset，并通过 install overwrite 语义更新 Project SkillAsset。
- [ ] 缺少 `SKILL.md`、非法 payload、重复 Project key 或 provider fetch 错误有稳定 400/409/500 映射。
- [ ] 相关测试覆盖 materializer、URL import 成功链路、invalid files 和 installed source 记录。
- [ ] 未修改 MCP install、外部 Marketplace UI、Skill 上传/手写创建入口。

## 开放问题

暂无阻塞问题。首期 version 可以从 `SKILL.md` metadata 派生或固定为 materializer 定义的远端导入版本；实现中必须固定规则并测试。
