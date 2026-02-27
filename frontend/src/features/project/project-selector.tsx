import { useState } from "react";
import type { Project, ProjectConfig } from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import { useCoordinatorStore } from "../../stores/coordinatorStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";

interface ProjectSelectorProps {
  projects: Project[];
  currentProjectId: string | null;
  onSelect: (id: string) => void;
}

export function ProjectSelector({ projects, currentProjectId, onSelect }: ProjectSelectorProps) {
  const [showCreate, setShowCreate] = useState(false);
  const [showConfig, setShowConfig] = useState(false);
  const { createProject, updateProjectConfig } = useProjectStore();
  const { backends } = useCoordinatorStore();
  const { workspacesByProjectId } = useWorkspaceStore();

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [backendId, setBackendId] = useState(backends[0]?.id ?? "");
  const [defaultAgentType, setDefaultAgentType] = useState("");
  const [defaultWorkspaceId, setDefaultWorkspaceId] = useState("");
  const [agentPresetsJson, setAgentPresetsJson] = useState("[]");
  const [configError, setConfigError] = useState<string | null>(null);
  const effectiveBackendId = backendId || backends[0]?.id || "";

  const handleCreate = async () => {
    if (!name.trim() || !effectiveBackendId) return;
    await createProject(name.trim(), description.trim(), effectiveBackendId);
    setName("");
    setDescription("");
    setBackendId("");
    setShowCreate(false);
  };

  const current = projects.find((p) => p.id === currentProjectId);
  const currentWorkspaces = currentProjectId
    ? (workspacesByProjectId[currentProjectId] ?? [])
    : [];

  const handleToggleConfig = () => {
    if (!current) return;
    if (!showConfig) {
      setDefaultAgentType(current.config.default_agent_type ?? "");
      setDefaultWorkspaceId(current.config.default_workspace_id ?? "");
      setAgentPresetsJson(JSON.stringify(current.config.agent_presets ?? [], null, 2));
      setConfigError(null);
    }
    setShowConfig(!showConfig);
  };

  const handleSaveConfig = async () => {
    if (!current) return;

    let parsedPresets: ProjectConfig["agent_presets"] = [];
    try {
      const parsed = JSON.parse(agentPresetsJson.trim() || "[]") as unknown;
      if (!Array.isArray(parsed)) {
        setConfigError("agent_presets 必须是数组");
        return;
      }
      parsedPresets = parsed.map((item, index) => {
        if (!item || typeof item !== "object") {
          throw new Error(`第 ${index + 1} 项必须是对象`);
        }
        const preset = item as Record<string, unknown>;
        const presetName = typeof preset.name === "string" ? preset.name.trim() : "";
        const presetAgentType = typeof preset.agent_type === "string" ? preset.agent_type.trim() : "";
        if (!presetName || !presetAgentType) {
          throw new Error(`第 ${index + 1} 项缺少 name 或 agent_type`);
        }
        return {
          name: presetName,
          agent_type: presetAgentType,
          config: (preset.config && typeof preset.config === "object")
            ? preset.config as Record<string, unknown>
            : {},
        };
      });
    } catch (error) {
      setConfigError(error instanceof Error ? error.message : "agent_presets JSON 解析失败");
      return;
    }

    const saved = await updateProjectConfig(current.id, {
      default_agent_type: defaultAgentType.trim() || null,
      default_workspace_id: defaultWorkspaceId || null,
      agent_presets: parsedPresets,
    });

    if (!saved) {
      setConfigError("配置保存失败，请稍后重试");
      return;
    }

    setConfigError(null);
    setShowConfig(false);
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between px-2">
        <p className="text-xs uppercase tracking-wider text-muted-foreground">项目</p>
        <div className="flex gap-1">
          <button
            type="button"
            onClick={handleToggleConfig}
            disabled={!current}
            className="rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-secondary hover:text-foreground disabled:opacity-50"
          >
            {showConfig ? "收起配置" : "配置"}
          </button>
          <button
            type="button"
            onClick={() => setShowCreate(!showCreate)}
            className="rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-secondary hover:text-foreground"
          >
            {showCreate ? "取消" : "+ 新建"}
          </button>
        </div>
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
            value={effectiveBackendId}
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
            disabled={!name.trim() || !effectiveBackendId}
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
          onClick={() => {
            setShowConfig(false);
            onSelect(project.id);
          }}
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

          {showConfig && (
            <div className="mt-2 space-y-2 rounded-md border border-border bg-background p-2">
              <p className="text-xs font-medium text-foreground">项目配置</p>
              <input
                value={defaultAgentType}
                onChange={(e) => setDefaultAgentType(e.target.value)}
                placeholder="默认 Agent 类型（如 claude-code）"
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              />
              <select
                value={defaultWorkspaceId}
                onChange={(e) => setDefaultWorkspaceId(e.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              >
                <option value="">默认 Workspace（可选）</option>
                {currentWorkspaces.map((ws) => (
                  <option key={ws.id} value={ws.id}>{ws.name}</option>
                ))}
              </select>
              <textarea
                value={agentPresetsJson}
                onChange={(e) => setAgentPresetsJson(e.target.value)}
                rows={6}
                placeholder='agent_presets JSON，例如 [{"name":"默认","agent_type":"claude-code","config":{}}]'
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-xs font-mono outline-none ring-ring focus:ring-1"
              />
              {configError && (
                <p className="text-xs text-destructive">{configError}</p>
              )}
              <button
                type="button"
                onClick={() => void handleSaveConfig()}
                className="w-full rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground"
              >
                保存配置
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
