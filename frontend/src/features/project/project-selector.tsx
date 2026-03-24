import { useState } from "react";
import { useNavigate } from "react-router-dom";
import type { Project, ProjectRole } from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import { useCoordinatorStore } from "../../stores/coordinatorStore";
import {
  DetailPanel,
  DetailSection,
} from "../../components/ui/detail-panel";

interface ProjectSelectorProps {
  projects: Project[];
  currentProjectId: string | null;
  onSelect: (id: string) => void;
}

interface ProjectCreateDrawerProps {
  open: boolean;
  onClose: () => void;
}

const PROJECT_ROLE_LABELS: Record<ProjectRole, string> = {
  owner: "Owner",
  editor: "Editor",
  viewer: "Viewer",
};

const PROJECT_VISIBILITY_LABELS: Record<Project["visibility"], string> = {
  private: "私有",
  template_visible: "模板可见",
};

function describeProjectAccess(project: Project): string {
  if (project.access.via_admin_bypass) {
    return "管理员旁路";
  }
  if (project.access.role) {
    return PROJECT_ROLE_LABELS[project.access.role];
  }
  if (project.access.via_template_visibility) {
    return "模板访客";
  }
  return "仅查看";
}

function ProjectCreateDrawer({ open, onClose }: ProjectCreateDrawerProps) {
  const { createProject, error } = useProjectStore();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");

  const handleCreate = async () => {
    if (!name.trim()) return;
    const created = await createProject(name.trim(), description.trim());
    if (!created) return;
    setName("");
    setDescription("");
    onClose();
  };

  return (
    <DetailPanel
      open={open}
      title="新建项目"
      subtitle="创建 Project"
      onClose={onClose}
      widthClassName="max-w-2xl"
    >
      <div className="space-y-4 p-5">
        <DetailSection title="基础信息">
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="项目名称"
            className="agentdash-form-input"
          />
          <input
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            placeholder="描述（可选）"
            className="agentdash-form-input"
          />
        </DetailSection>

        {error && <p className="text-xs text-destructive">创建失败：{error}</p>}

        <div className="flex items-center justify-end border-t border-border pt-3">
          <button
            type="button"
            onClick={() => void handleCreate()}
            disabled={!name.trim()}
            className="agentdash-button-primary"
          >
            创建项目
          </button>
        </div>
      </div>
    </DetailPanel>
  );
}

export function ProjectSelector({
  projects,
  currentProjectId,
  onSelect,
}: ProjectSelectorProps) {
  const navigate = useNavigate();
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [focusedProjectId, setFocusedProjectId] = useState<string | null>(null);
  const { backends } = useCoordinatorStore();

  return (
    <>
      <div className="space-y-2 rounded-[12px] border border-border bg-secondary/35 p-2.5">
        <div className="flex items-center justify-between px-1">
          <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">项目</p>
          <button
            type="button"
            onClick={() => setIsCreateOpen(true)}
            className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            + 新建
          </button>
        </div>

        {projects.length === 0 && (
          <p className="rounded-[10px] border border-dashed border-border px-3 py-3 text-sm text-muted-foreground">暂无项目</p>
        )}

        {projects.map((project) => {
          const isActive = currentProjectId === project.id;
          const isFocused = focusedProjectId === project.id;
          const showSettingsButton = isActive || isFocused;

          return (
            <div
              key={project.id}
              className={`flex items-center justify-between rounded-[10px] border px-3 py-2.5 text-sm transition-colors ${
                isActive
                  ? "border-primary/20 bg-background"
                  : "border-transparent bg-transparent hover:border-border hover:bg-background/80"
              }`}
              onMouseEnter={() => setFocusedProjectId(project.id)}
              onMouseLeave={() => setFocusedProjectId((value) => (value === project.id ? null : value))}
              onFocusCapture={() => setFocusedProjectId(project.id)}
              onBlurCapture={(event) => {
                const nextTarget = event.relatedTarget as Node | null;
                if (!nextTarget || !event.currentTarget.contains(nextTarget)) {
                  setFocusedProjectId((value) => (value === project.id ? null : value));
                }
              }}
            >
              <button
                type="button"
                onClick={() => onSelect(project.id)}
                className="min-w-0 flex-1 text-left text-foreground"
              >
                <p className="truncate font-medium">{project.name}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {project.description || `ID: ${project.id}`}
                </p>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
                    {describeProjectAccess(project)}
                  </span>
                  {project.is_template && (
                    <span className="rounded-full border border-amber-200 bg-amber-50 px-2 py-0.5 text-[10px] text-amber-700">
                      模板
                    </span>
                  )}
                  <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
                    {PROJECT_VISIBILITY_LABELS[project.visibility]}
                  </span>
                </div>
              </button>

              {showSettingsButton && (
                <button
                  type="button"
                  onClick={() => {
                    onSelect(project.id);
                    navigate(`/projects/${project.id}/settings`);
                  }}
                  className="ml-2 inline-flex h-7 w-7 items-center justify-center rounded-[8px] border border-border bg-secondary text-sm leading-none text-muted-foreground transition-colors hover:text-foreground"
                  aria-label="打开项目设置"
                  title="打开项目设置"
                >
                  ⋯
                </button>
              )}
            </div>
          );
        })}
      </div>

      <ProjectCreateDrawer
        key={`project-create-${isCreateOpen ? "open" : "closed"}-${backends[0]?.id ?? "none"}`}
        open={isCreateOpen}
        onClose={() => setIsCreateOpen(false)}
      />
    </>
  );
}
