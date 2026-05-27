# Design — Extension Assets Frontend Surface

## 1. 范围与边界

本任务跨三层：
- **Backend**：补 uninstall route + application 函数 + repo `delete` 方法 + 契约 DTO
- **Contracts**：扩展 `extension-runtime-contracts` 重新生成
- **Frontend**：新增 Assets 类目 + service + panel + 上传/安装/卸载 UI

不动 Extension Runtime 的运行时投影计算逻辑、不动 Marketplace 安装路径、不动 Canvas Promote 路径、不动桌面壳 hosting 行为。

## 2. 后端：Uninstall

### 2.1 Repo 扩展

`crates/agentdash-domain/src/shared_library/project_extension.rs`：在 `ProjectExtensionInstallationRepository` 添加：

```rust
async fn delete(
    &self,
    project_id: Uuid,
    installation_id: Uuid,
) -> Result<bool, DomainError>;
```

返回 `Ok(true)` 表示删除成功；`Ok(false)` 表示该 `(project_id, installation_id)` 不存在（NOT FOUND，由 application 层翻译为 404）。这与现有 `get_by_*` 的 Option 风格一致。

### 2.2 Postgres 实现

`crates/agentdash-infrastructure/src/persistence/postgres/project_extension_installation_repository.rs`：实现 `delete`，单条 SQL：

```sql
DELETE FROM project_extension_installations
 WHERE id = $1 AND project_id = $2
```

返回 `rows_affected > 0`。**不**级联删除 `extension_package_artifacts`（归档保留）。配 1 条 pg test 覆盖 happy path + cross-project 不命中。

### 2.3 Application 层

新建 `crates/agentdash-application/src/extension_runtime.rs` 顶部（或并入现有同文件，已存在）：

```rust
pub struct UninstallExtensionInstallationInput {
    pub project_id: Uuid,
    pub installation_id: Uuid,
}

pub struct UninstallExtensionInstallationOutput {
    pub installation_id: Uuid,
    pub extension_key: String,
}

pub async fn uninstall_extension_installation(
    repos: &RepositorySet,
    input: UninstallExtensionInstallationInput,
) -> Result<UninstallExtensionInstallationOutput, DomainError>;
```

实现：先 `list_by_project` 查找匹配 id（拿 `extension_key`），再 `delete`。如果 `delete` 返回 `false`，返回 `DomainError::NotFound { entity: "project_extension_installation", ... }`。`list_by_project` 的开销可以接受；如果未来 N 大可改为加 `get_by_id` 方法，超出本任务范围。

> **替代**：直接给 repo 加 `get_by_id(project_id, installation_id)` 一次查询，再 delete。我倾向后者，避免遍历列表。最终方案：**加一个 `get_by_project_and_id` 单查询方法**，与 install 路径上 `get_by_project_and_digest` 风格一致。

### 2.4 Route

`crates/agentdash-api/src/routes/extension_runtime.rs`：

```rust
pub async fn uninstall_extension_installation_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionInstallationPath>,
) -> Result<Json<UninstallExtensionInstallationResponse>, ApiError>;
```

`ProjectExtensionInstallationPath { project_id: String, installation_id: String }`。注册：

```rust
.route(
    "/projects/{project_id}/extensions/{installation_id}",
    delete(extension_runtime::uninstall_extension_installation_route),
)
```

权限 `ProjectPermission::Edit`。

### 2.5 Contract DTO

`crates/agentdash-contracts/src/extension_runtime.rs`：

```rust
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct UninstallExtensionInstallationResponse {
    pub installation_id: String,
    pub extension_key: String,
}
```

跑 `cargo run -p agentdash-contracts --bin generate_contracts_ts` 生成 `extension-runtime-contracts.ts`。

## 3. 前端：Service 层

### 3.1 新建 `packages/app-web/src/services/extensionPackage.ts`

封装四条 API：

```ts
export async function listExtensionArtifacts(projectId: string): Promise<ExtensionPackageArtifactResponse[]>;

export async function uploadExtensionArtifact(
  projectId: string,
  file: File,                   // 浏览器拿到的归档文件
  archiveDigest: string,        // sha256:<hex>，前端计算
): Promise<ExtensionPackageArtifactResponse>;

export async function installExtensionArtifact(
  projectId: string,
  artifactId: string,
  body: InstallExtensionPackageArtifactRequest,
): Promise<ExtensionPackageInstallationResponse>;

export async function downloadExtensionArtifact(
  projectId: string,
  artifactId: string,
): Promise<Blob>;
```

实现细节：
- `uploadExtensionArtifact` 必须用 `fetch` 直发 `multipart/form-data`，不能走 `api.post`（后者是 JSON）。`Authorization` 头复用 `api/client` 暴露的 token 取法。
- `downloadExtensionArtifact` 用 `fetch` + `response.blob()`，错误体仍按 `api/client` 的错误风格抛。
- mapper 走 `recordOrThrow` + `requireStringField` 风格，与 `services/extensionRuntime.ts` 一致。

### 3.2 扩展 `services/extensionRuntime.ts`

加一个：

```ts
export async function uninstallExtensionInstallation(
  projectId: string,
  installationId: string,
): Promise<UninstallExtensionInstallationResponse>;
```

直接 `api.delete<...>` 即可。

### 3.3 SHA-256 工具

新建 `packages/app-web/src/utils/sha256.ts`：

```ts
export async function sha256OfBlob(blob: Blob): Promise<string> {
  const buffer = await blob.arrayBuffer();
  const hash = await crypto.subtle.digest("SHA-256", buffer);
  return "sha256:" + Array.from(new Uint8Array(hash))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}
```

配单测（vitest 已有 jsdom + crypto.subtle 支持，必要时用 `@vitest/web-worker` 或换成 node `crypto`）。如果 jsdom 不支持，退化为在测试里 mock。

## 4. 前端：Assets 类目接入

### 4.1 `AssetsTabView.tsx`

`SHAREABLE_CATEGORIES` 新增：

```ts
{ segment: "extension", label: "Extension", hint: "本地打包扩展（含已安装与归档）" },
```

放在 `vfs-mount` 之后。

### 4.2 路由声明

定位 `dashboard/assets/:category` 子路由的注册位置（在 router 配置文件里），加 `path: "extension"` → `<ExtensionCategoryPanel />`。

## 5. 前端：`ExtensionCategoryPanel`

### 5.1 文件结构

```
packages/app-web/src/features/assets-panel/categories/
  ExtensionCategoryPanel.tsx              # 主面板（已安装段 + 归档段 + notice）
  extension/
    InstalledExtensionRow.tsx             # 已安装一行（可展开详情）
    ExtensionArtifactRow.tsx              # 归档一行
    UploadExtensionDialog.tsx             # 上传 → digest → 上传
    InstallFromArtifactDialog.tsx         # 输入 extension_key/display_name/overwrite
    UninstallConfirmDialog.tsx            # 二次确认
    extensionAggregation.ts               # 把 projection 按 extension_key join 到 row VM
    extensionAggregation.test.ts
```

### 5.2 状态

`ExtensionCategoryPanel` 内本地 state：

```ts
const [artifacts, setArtifacts] = useState<ExtensionPackageArtifactResponse[]>([]);
const [artifactsLoading, setArtifactsLoading] = useState(false);
const [notice, setNotice] = useState<NoticeData | null>(null);
const [busy, setBusy] = useState<{ kind: "upload" } | { kind: "install"; artifactId: string } | { kind: "uninstall"; installationId: string } | null>(null);
const [dialog, setDialog] = useState<DialogState>({ kind: "closed" });
```

extension projection 来自 `useProjectExtensionRuntime(projectId)`（已存在 hook）。归档列表自管。

### 5.3 行 VM 聚合

`extensionAggregation.ts` 输出：

```ts
interface InstalledExtensionRowVM {
  installation: ExtensionInstallationProjectionResponse;
  source: "marketplace" | "local_archive" | "marketplace_with_archive" | "unknown";
  version: string;
  permissions: ExtensionPermissionDeclarationResponse[];
  workspaceTabs: ExtensionWorkspaceTabProjectionResponse[];
  runtimeActions: ExtensionRuntimeActionProjectionResponse[];
  commands: ExtensionCommandProjectionResponse[];
  flags: ExtensionFlagProjectionResponse[];
  messageRenderers: ExtensionMessageRendererProjectionResponse[];
  bundle: ExtensionBundleProjectionResponse | null;
}

export function aggregateInstalledExtensions(
  projection: ExtensionRuntimeProjectionResponse,
): InstalledExtensionRowVM[];
```

按 `extension_key` 分组。该函数纯函数，配 vitest 测试覆盖 4 种 source 分支。

### 5.4 写操作流（统一模式）

```
[trigger] -> setBusy(...) -> service call -> on success:
  - setNotice(success)
  - 局部刷新（artifacts: 重新拉；installations: useExtensionRuntimeStore.getState().fetchProject(projectId)）
  - setBusy(null)
on failure:
  - setNotice(error from server text)
  - setBusy(null)
```

### 5.5 上传 dialog 流

1. 用户点「上传归档」
2. 文件 picker，accept `.tgz,.gz,application/gzip,application/x-gzip,application/vnd.agentdash.extension+gzip`
3. 校验：扩展名（`.tgz` 或 `.agentdash-extension.tgz`），字节数 < 50 * 1024 * 1024
4. 计算 sha256（loading state，长归档可能要几百 ms）
5. POST `multipart/form-data`：`archive` (file) + `archive_digest` (string)
6. 成功后弹 `InstallFromArtifactDialog`，预填 `extension_id` 为 `display_name` 默认值，其它由用户决定

### 5.6 下载

```ts
const blob = await downloadExtensionArtifact(projectId, artifactId);
const url = URL.createObjectURL(blob);
const a = document.createElement("a");
a.href = url;
a.download = `${row.extensionId}-${row.packageVersion}.agentdash-extension.tgz`;
a.click();
URL.revokeObjectURL(url);
```

### 5.7 卸载流

弹 `UninstallConfirmDialog`，文案：
> 卸载 `<display_name>` (`<extension_key>`)？这会移除项目下该扩展的安装记录。**归档保留**，可随时从「归档」段重新安装。

## 6. UI / UX 草图

```
┌─ Extension ────────────────────────────────────────────────┐
│ [上传归档]                            筛选: [全部 v]       │
│ ───── 已安装 (3) ──────────────────────────────────────── │
│  ▸ Local Hello   local-hello@0.1.0   [本地归档]           │
│       2 tabs · 1 action · 1 permission   [下载] [卸载]    │
│  ▸ Canvas Hello  canvas-hello@0.2.0  [本地归档]           │
│  ▸ Onboarding    onboarding@1.4.0    [Marketplace]        │
│ ───── 归档库 (2) ───────────────────────────────────────── │
│   local-hello      0.1.0   sha256:2963…  124 KB  10:32   │
│       [从归档安装] [下载]                                  │
│   canvas-hello     0.2.0   sha256:9aa1…  88 KB   昨日     │
│ ─────────────────────────────────────────────────────────  │
└────────────────────────────────────────────────────────────┘
```

样式沿用 `MarketplaceCategoryPanel` 的 token / 间距 / `agentdash-form-input` / `notice` 组件，不引新 primitive。

## 7. 测试计划

### Backend

- `extension_runtime` application：`uninstall_extension_installation` 单测：happy path、cross-project 404、不存在 404
- `routes::extension_runtime` integration：`DELETE /projects/{p}/extensions/{i}` 200 + 404 + 401/403
- Postgres repo `delete` 测试：rows_affected 行为
- 契约重新生成后 `pnpm run contracts:check`

### Frontend

- `services/extensionPackage.test.ts`：list / upload mapper / install mapper / 错误体翻译
- `utils/sha256.test.ts`
- `extensionAggregation.test.ts`：4 种 source 分支
- `ExtensionCategoryPanel.test.tsx`：渲染态（empty / 仅有归档 / 有已安装 / 有 notice）+ 上传按钮 disabled 在 busy 期间
- E2E `tests/e2e/extension-assets-panel.spec.ts`：UI 走完上传 → 安装 → 卸载

## 8. 兼容性 / 回滚

- 后端 `delete` 方法是新增能力，不动现有签名；contract 新增 `UninstallExtensionInstallationResponse` 是新类型，不破坏。
- 路由新增 `DELETE`，不动现有 `POST /install`、`GET/POST /extension-artifacts`。
- 前端新增类目，不动其它 categories；如果 extension service / panel 出错最多导致该类目不可用，不影响其他 Assets 类目。
- 回滚：直接 revert 本任务的几个 commit；后端在客户端版本错位时，老前端不调 DELETE 路由，新前端调老后端会 405，可接受。

## 9. 开放问题

- **批量卸载 / 批量删除归档**：不在范围。
- **删除归档**：一旦支持，需要校验"该归档是否仍被某 installation 引用"。本任务不做。
- **Marketplace + 本地归档混合源 row 的 UX**：当前以 source badge 区分；如果未来双源共存高频，可加二级标签，本任务先做最简。
