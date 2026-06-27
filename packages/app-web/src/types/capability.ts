export type CapabilityKey =
  | "file_read"
  | "file_write"
  | "shell_execute"
  | "workflow"
  | "collaboration"
  | "task"
  | "workspace_module";

export type CapabilityGroup = "basic" | "extended";

export interface CapabilityOption {
  value: CapabilityKey;
  label: string;
  description: string;
  group: CapabilityGroup;
}

export const CAPABILITY_GROUPS: Array<{ key: CapabilityGroup; label: string }> = [
  { key: "basic", label: "基础能力" },
  { key: "extended", label: "扩展能力" },
];

export const CAPABILITY_OPTIONS: CapabilityOption[] = [
  { value: "file_read", label: "只读访问", description: "文件读取、目录列表、搜索", group: "basic" },
  { value: "file_write", label: "文件写入", description: "文件写入、补丁应用", group: "basic" },
  { value: "shell_execute", label: "命令执行", description: "Shell 命令执行", group: "basic" },
  { value: "workflow", label: "工作流", description: "Workflow 产出汇报", group: "extended" },
  { value: "collaboration", label: "协作", description: "Companion 派发、回传、Hook 审核", group: "extended" },
  { value: "task", label: "Task", description: "Task 读取与维护", group: "extended" },
  { value: "workspace_module", label: "Workspace Module", description: "模块创建、调用与展示，包含 Canvas", group: "extended" },
];
