# Implement · Skill Catalog Source 导入闭环

## 执行步骤

1. Research existing import/install code
   - 确认 `SkillAssetService::import_remote`、`RemoteSkillSource`、Shared Library `install_library_asset_to_project` 的当前职责。
   - 确认 `SkillTemplatePayload` 与 `SkillAsset` install mapper 的字段要求。

2. Materializer
   - 抽出远端 Skill files -> `SkillTemplatePayload` 的 application helper。
   - 复用现有 content typing、metadata parsing、`validate_skill_files` 和 digest 计算。
   - 固定 URL import 的 `source_ref` 和 version 规则。

3. Import use case
   - 新增 `import_remote_skill_url_to_project` 或等价 helper。
   - fetch URL 后写入 `LibraryAsset(source=remote_imported)`。
   - 调用 Shared Library install 生成 Project SkillAsset。
   - 处理同源更新、其它来源冲突和 install overwrite 策略。

4. API route
   - `POST /api/projects/{project_id}/skill-assets/import` 保持 request/response 不变。
   - route 调用新的 use case，不再直接 `SkillAssetService::import_remote` 创建 Project SkillAsset。

5. Tests
   - materializer 单测。
   - application import 成功链路测试。
   - invalid files / missing `SKILL.md` 测试。
   - installed source 记录测试。

## 主要文件

- `crates/agentdash-application/src/skill_asset/service.rs`
- `crates/agentdash-application/src/shared_library/...`
- `crates/agentdash-api/src/routes/skill_assets.rs`
- `crates/agentdash-domain/src/shared_library/value_objects.rs`
- `crates/agentdash-domain/src/skill_asset/...`
- `crates/agentdash-infrastructure/src/skill_source/http.rs`

## 验证命令

```powershell
cargo fmt --check
cargo test -p agentdash-application skill_asset
cargo test -p agentdash-application shared_library
cargo test -p agentdash-api skill_assets
pnpm run migration:guard
git diff --check
```

前端 typecheck 仅在 `node_modules` 可用时运行：

```powershell
pnpm run frontend:check
```

## 风险点

- route 为保持返回 `SkillAssetResponse`，use case 必须返回 Project SkillAsset，而不是 LibraryAsset。
- 现有 UI 可能依赖 `SkillAsset.source=github` 做 badge；本 child 应同步调整最小必要的 source display，使 installed source 成为远端来源事实。
- Shared Library install 的 overwrite 策略如果默认拒绝同 key，应在 URL import use case 中固定策略并测试。
- 不要把远端 URL 作为 Project SkillAsset source 字段的新事实；URL identity 属于 LibraryAsset source_ref / InstalledAssetSource。

## 交付检查

- [ ] URL Import 写入 Shared Library `skill_template`。
- [ ] URL Import 安装 Project SkillAsset 并写入 `InstalledAssetSource`。
- [ ] 现有 fetch / file validation 能力保留。
- [ ] 测试覆盖成功、非法文件、重复导入和 source identity。
- [ ] 未修改 MCP install、Marketplace 外部来源 UI、Skill 手写创建/上传入口。
