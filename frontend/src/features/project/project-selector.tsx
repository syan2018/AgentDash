import { useState } from "react";
import type { Project } from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import { useCoordinatorStore } from "../../stores/coordinatorStore";

interface ProjectSelectorProps {
  projects: Project[];
  currentProjectId: string | null;
  onSelect: (id: string) => void;
}

export function ProjectSelector({ projects, currentProjectId, onSelect }: ProjectSelectorProps) {
  const [showCreate, setShowCreate] = useState(false);
  const { createProject } = useProjectStore();
  const { backends } = useCoordinatorStore();

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [backendId, setBackendId] = useState(backends[0]?.id ?? "");

  const handleCreate = async () => {
    if (!name.trim() || !backendId) return;
    await createProject(name.trim(), description.trim(), backendId);
    setName("");
    setDescription("");
    setShowCreate(false);
  };

  const current = projects.find((p) => p.id === currentProjectId);

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between px-2">
        <p className="text-xs uppercase tracking-wider text-muted-foreground">项目</p>
        <button
          type="button"
          onClick={() => setShowCreate(!showCreate)}
          className="rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-secondary hover:text-foreground"
        >
          {showCreate ? "取消" : "+ 新建"}
        </button>
      </div>

      {showCreate && (
        <div className="space-y-2 rounded-md border border-border bg-background p-2">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="项目名称"
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
          <input
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="描述（可选）"
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
          <select
            value={backendId}
            onChange={(e) => setBackendId(e.target.value)}
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          >
            <option value="">选择后端</option>
            {backends.map((b) => (
              <option key={b.id} value={b.id}>{b.name}</option>
            ))}
          </select>
          <button
            type="button"
            onClick={() => void handleCreate()}
            disabled={!name.trim() || !backendId}
            className="w-full rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
          >
            创建项目
          </button>
        </div>
      )}

      {projects.length === 0 && !showCreate && (
        <p className="px-2 py-2 text-sm text-muted-foreground">暂无项目</p>
      )}

      {projects.map((project) => (
        <button
          key={project.id}
          type="button"
          onClick={() => onSelect(project.id)}
          className={`w-full rounded-md px-3 py-2 text-left text-sm transition-colors ${
            currentProjectId === project.id
              ? "bg-primary text-primary-foreground"
              : "text-foreground hover:bg-secondary"
          }`}
        >
          <p className="truncate font-medium">{project.name}</p>
          {project.description && (
            <p className="truncate text-xs opacity-75">{project.description}</p>
          )}
        </button>
      ))}

      {current && (
        <div className="border-t border-border px-2 pt-2">
          <p className="text-[10px] text-muted-foreground">后端: {current.backend_id}</p>
        </div>
      )}
    </div>
  );
}
