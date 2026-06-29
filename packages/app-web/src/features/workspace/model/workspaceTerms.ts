import type {
  BackendWorkspaceInventory,
  ProjectBackendAccess,
  WorkspaceBindingStatus,
  WorkspaceIdentityKind,
  WorkspaceStatus,
} from "../../../types";

/**
 * 面向用户的中文文案单一事实源。
 *
 * 设计原则（见任务 design.md D2）：
 * - 中文为主，仅保留少量约定俗成专名（Workspace、Backend）不翻译。
 * - 内部模型词统一翻译为用户词表，避免界面直出实现 jargon。
 * - 调试信息（identity JSON / detected_facts）通过文案降级收纳到「高级」区。
 */

/** 保留不翻译的专名（与 Backend Access 面板、团队口语一致）。 */
export const PROPER_NOUNS = {
  workspace: "Workspace",
  backend: "Backend",
} as const;

/**
 * 用户词表：内部模型词 -> 面向用户的中文表达。
 * 在文案中引用这些常量，避免散落硬编码导致术语漂移。
 */
export const TERMS = {
  /** binding / 落点 */
  binding: "目录绑定",
  /** inventory / candidate / 候选项 / 发现项 */
  inventory: "可选目录",
  /** identity */
  identity: "代码来源",
  /** resolution */
  resolution: "目录解析",
} as const;

/** 代码来源（identity_kind）的中文 label。 */
export const IDENTITY_KIND_LABELS: Record<WorkspaceIdentityKind, string> = {
  git_repo: "Git 仓库",
  p4_workspace: "P4 工作空间",
  local_dir: "本地目录",
};

/** Workspace 状态徽章 label。 */
export const WORKSPACE_STATUS_LABELS: Record<WorkspaceStatus, string> = {
  pending: "待完善",
  ready: "可用",
  active: "使用中",
  archived: "已归档",
  error: "异常",
};

/** 目录绑定（binding）状态 label。 */
export const BINDING_STATUS_LABELS: Record<WorkspaceBindingStatus, string> = {
  pending: "未确认",
  ready: "可用",
  offline: "离线",
  error: "异常",
};

/** 目录解析（resolution）状态徽章 label。 */
export const RESOLUTION_STATE_LABELS: Record<"resolved" | "warning" | "blocked", string> = {
  resolved: "目录就绪",
  warning: "需注意",
  blocked: "目录不可用",
};

/** Backend 授权（access）状态 label。 */
export const BACKEND_ACCESS_STATUS_LABELS: Record<ProjectBackendAccess["status"], string> = {
  active: "已启用",
  paused: "已暂停",
  revoked: "已撤销",
};

/** 机器上可用目录（inventory）快照状态 label。 */
export const INVENTORY_STATUS_LABELS: Record<BackendWorkspaceInventory["status"], string> = {
  available: "可用",
  stale: "过期",
  offline: "离线",
  error: "异常",
};
