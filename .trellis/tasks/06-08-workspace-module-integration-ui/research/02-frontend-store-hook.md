# Research: 前端 extension-runtime 消费（workspace-module store/hook 样板）

- **Query**: useProjectExtensionRuntime / extensionRuntimeStore / fetchProject / API client / 生成类型
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### Files Found

| File Path | Description |
|---|---|
| `packages/app-web/src/features/extension-runtime/model/useProjectExtensionRuntime.ts` | React hook：订阅 store + lazy fetch |
| `packages/app-web/src/features/extension-runtime/model/extensionRuntimeStore.ts` | zustand store：`byProjectId` + `fetchProject` + inflight 去重 |
| `packages/app-web/src/features/extension-runtime/model/types.ts` | 状态类型 + `empty*Projection()` / `idle*State()` |
| `packages/app-web/src/services/extensionRuntime.ts` | API client 封装（`api.get/post/delete`） |
| `packages/app-web/src/features/workspace-runtime/model/types.ts:19-26` | `ProjectExtensionRuntimeState` / `*Status` 的 canonical 定义（被 model/types.ts re-export） |
| `packages/app-web/src/generated/workspace-module-contracts.ts` | 生成的 WorkspaceModule TS 类型（见 04 文档生成机制） |

### API client（services/extensionRuntime.ts）

API client 是 `import { api } from "../api/client"`，用法：
```ts
import { api } from "../api/client";
export async function fetchProjectExtensionRuntime(projectId: string): Promise<ExtensionRuntimeProjectionResponse> {
  return api.get<ExtensionRuntimeProjectionResponse>(
    `/projects/${encodeURIComponent(projectId)}/extension-runtime`,
  );
}
```
- `api.get/post/delete<T>(path, body?)`，path 是相对 `/api` 下（无需 host 前缀；host 拼接见 `../api/origin` 的 `buildApiPath`，仅 webview 资产 URL 用，行 49-62）。
- 生成类型从 `../generated/extension-runtime-contracts` 导入（行 3-10）。新 store 对应导入 `../generated/workspace-module-contracts`（类型名见下）。

### zustand store 样板（extensionRuntimeStore.ts）

- `create<State>()((set) => ({ byProjectId: {}, async fetchProject(projectId) {...}, resetProject(...) }))`。
- 关键模式：
  - `inflight = new Map<string, Promise<void>>()` 模块级 map 做并发去重（行 13、37-41、81）。
  - `loadingState()`：已 ready → `"refreshing"`，否则 `"loading"`（行 19-29）。
  - 成功 set `status:"ready"`，失败 set `status:"error"` 并保留旧 projection（行 64-76）。
  - 额外导出 `selectProjectExtensionRuntimeState(projectId)`（行 97）做非 hook 读取。

### hook 样板（useProjectExtensionRuntime.ts）

- `useExtensionRuntimeStore((state) => projectId ? state.byProjectId[projectId] : null)` 订阅切片。
- `useEffect` 内对非 ready/loading/refreshing 状态触发 `fetchProject`（行 15-22）；用 `getState()` 读当前避免重复请求。
- 返回 fallback `idleProjectExtensionRuntimeState()`（projectId 为空）或带 projectId 的 idle。

### 状态类型（features/workspace-runtime/model/types.ts）

- `ProjectExtensionRuntimeStatus = "idle" | "loading" | "ready" | "refreshing" | "error"`（行 19）。
- `ProjectExtensionRuntimeState { project_id, status, projection, error }`（行 21-26）。
- 这是 canonical 位置；`extension-runtime/model/types.ts:1-7` 仅 re-export 之，并提供 `emptyExtensionRuntimeProjection()`（含 10 个空数组）/`idleProjectExtensionRuntimeState()`。
- WorkspaceModule store 建议同样在 `workspace-runtime/model/types.ts` 定义状态类型，feature store 复用，保持单一位置。

### WorkspaceModule 生成类型（消费）

`workspace-module-contracts.ts` 导出（见 `generate_ts.rs:498-514`）：
`WorkspaceModuleKind`, `WorkspaceModuleStatusKind`, `WorkspaceModuleStatus`, `WorkspaceModuleSummary`,
`WorkspaceModuleUiEntry`, `WorkspaceModuleOperationDispatch`, `WorkspaceModuleOperation`, `WorkspaceModuleDescriptor`。
（这些经 `packages/app-web/src/types` barrel 统一再导出，参照 model/types.ts:1 的 `from "../../../types"` 用法。）

## Caveats / Not Found

- `api` client 具体实现（`api/client.ts`）未深读；用法是 `api.get<T>(path)`，与 extensionRuntime.ts 一致即可。
- 新 store 的"空 projection"应是 `[]`（WorkspaceModule 是数组，非含 10 字段的对象），比 extension-runtime 简单。
