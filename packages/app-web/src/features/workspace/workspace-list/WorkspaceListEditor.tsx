/**
 * 薄壳：按 mode 分发到 create / detail 抽屉，并保留对外兼容导出。
 *
 * 历史上本文件承载了 1000+ 行 create/detail/detect/register/badges 实现；
 * P2 重构已拆分为聚焦文件：
 * - badges.tsx：三个状态徽章
 * - DirectoryDetector.tsx：可复用的「选 Backend + 浏览 + 识别 + 登记」单元
 * - IdentityFields.tsx / CandidateList.tsx：代码来源字段与可选目录列表
 * - WorkspaceCreateDrawer.tsx / WorkspaceDetailDrawer.tsx：两种模式各自的抽屉
 * 此处仅保留 `WorkspaceEditorDrawer` 薄壳与徽章 re-export，避免破坏既有 import。
 */
import type {
  ProjectBackendAccess,
  Workspace,
  WorkspaceInventoryCandidate,
} from "../../../types";
import { WorkspaceCreateDrawer } from "./WorkspaceCreateDrawer";
import { WorkspaceDetailDrawer } from "./WorkspaceDetailDrawer";

export { WorkspaceStatusBadge, BindingStatusBadge, ResolutionBadge } from "./badges";

interface WorkspaceEditorDrawerProps {
  open: boolean;
  projectId: string;
  mode: "create" | "detail";
  workspace: Workspace | null;
  candidates: WorkspaceInventoryCandidate[];
  accesses: ProjectBackendAccess[];
  canManageBindings: boolean;
  onClose: () => void;
  onSetDefault?: (workspaceId: string | null) => void;
  onCandidatesChanged: () => void | Promise<void>;
  onInventoryChanged?: () => void | Promise<void>;
}

export function WorkspaceEditorDrawer({
  open,
  projectId,
  mode,
  workspace,
  candidates,
  accesses,
  canManageBindings,
  onClose,
  onSetDefault,
  onCandidatesChanged,
  onInventoryChanged,
}: WorkspaceEditorDrawerProps) {
  if (mode === "create") {
    return (
      <WorkspaceCreateDrawer
        open={open}
        projectId={projectId}
        candidates={candidates}
        accesses={accesses}
        canManageBindings={canManageBindings}
        onClose={onClose}
        onSetDefault={onSetDefault}
        onCandidatesChanged={onCandidatesChanged}
        onInventoryChanged={onInventoryChanged}
      />
    );
  }

  if (!workspace) return null;

  return (
    <WorkspaceDetailDrawer
      open={open}
      projectId={projectId}
      workspace={workspace}
      candidates={candidates}
      accesses={accesses}
      canManageBindings={canManageBindings}
      onClose={onClose}
      onCandidatesChanged={onCandidatesChanged}
      onInventoryChanged={onInventoryChanged}
    />
  );
}
