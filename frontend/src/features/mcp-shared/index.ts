/**
 * MCP 共享 UI 组件（跨 feature 复用）。
 *
 * 目前仅导出单条 MCP Server Decl 的编辑器；如后续有 MCP 相关通用 UI
 * （如 badge / preview card）可在此继续收敛。
 */

export {
  McpServerDeclEditor,
  KeyValueList,
  StringList,
} from "./McpServerDeclEditor";
export type { McpServerDeclEditorProps } from "./McpServerDeclEditor";
export { createDefaultMcpServerDecl } from "./helpers";
