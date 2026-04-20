/**
 * Assets Panel feature 公共入口。
 *
 * 对齐项目 FSD 风格：仅从本目录 `index.ts` re-export 组件，
 * 外部（App.tsx 路由 / workspace-layout NavLink 等）只依赖本模块入口。
 */

export { AssetsTabView } from "./AssetsTabView";
export { WorkflowCategoryPanel } from "./categories/WorkflowCategoryPanel";
export { CanvasCategoryPanel } from "./categories/CanvasCategoryPanel";
export { McpPresetCategoryPanel } from "./categories/McpPresetCategoryPanel";
