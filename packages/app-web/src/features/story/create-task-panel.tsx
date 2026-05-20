import { useCallback, useState } from "react";
import { Button, Field, Select, TextInput, Textarea } from "@agentdash/ui";
import type {
  AgentBinding,
  ContextSourceRef,
  ProjectConfig,
  Story,
  Workspace,
} from "../../types";
import { AgentBindingFields } from "../task/agent-binding-fields";
import {
  createDefaultAgentBinding,
  hasAgentBindingSelection,
  normalizeAgentBinding,
  resolveDefaultWorkspaceId,
} from "../task/agent-binding";
import { useStoryStore } from "../../stores/storyStore";
import { Tooltip } from "../../components/ui/tooltip";

import { sourceKindMeta } from "./context-source-utils";

interface CreateTaskPanelProps {
  story: Story;
  storyId: string;
  workspaces: Workspace[];
  projectConfig?: ProjectConfig;
  onCreated: () => void;
}

export function CreateTaskPanel({
  story,
  storyId,
  workspaces,
  projectConfig,
  onCreated,
}: CreateTaskPanelProps) {
  const { createTask, error } = useStoryStore();
  const [isExpanded, setIsExpanded] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [workspaceId, setWorkspaceId] = useState(() => resolveDefaultWorkspaceId(projectConfig, workspaces));
  const [agentBinding, setAgentBinding] = useState<AgentBinding>(() => createDefaultAgentBinding(projectConfig));
  const [selectedContextIndexes, setSelectedContextIndexes] = useState<number[]>([]);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const availableContexts = story.context.source_refs;
  const defaultWorkspaceId = resolveDefaultWorkspaceId(projectConfig, workspaces);

  const openPanel = useCallback(() => {
    setWorkspaceId(defaultWorkspaceId);
    setAgentBinding(createDefaultAgentBinding(projectConfig));
    setSelectedContextIndexes([]);
    setFormMessage(null);
    setIsExpanded(true);
  }, [defaultWorkspaceId, projectConfig]);

  const closePanel = useCallback(() => {
    setIsExpanded(false);
    setSelectedContextIndexes([]);
    setFormMessage(null);
  }, []);

  const toggleContextSelection = useCallback((index: number) => {
    setSelectedContextIndexes((current) =>
      current.includes(index) ? current.filter((item) => item !== index) : [...current, index].sort((a, b) => a - b),
    );
  }, []);

  const handleSubmit = async () => {
    if (!title.trim()) return;
    if (!hasAgentBindingSelection(agentBinding, projectConfig)) {
      setFormMessage("请指定 Agent 类型或预设，或先在 Project 配置中设置 default_agent_type");
      return;
    }
    setIsSubmitting(true);
    setFormMessage(null);
    try {
      const selectedContexts = selectedContextIndexes
        .map((index) => availableContexts[index])
        .filter((item): item is ContextSourceRef => Boolean(item));
      const task = await createTask(storyId, {
        title: title.trim(),
        description: description.trim() || undefined,
        workspace_id: workspaceId || null,
        agent_binding: normalizeAgentBinding({
          ...agentBinding,
          context_sources: selectedContexts,
        }),
      });
      if (!task) return;

      onCreated();
      setTitle("");
      setDescription("");
      setWorkspaceId(defaultWorkspaceId);
      setAgentBinding(createDefaultAgentBinding(projectConfig));
      setSelectedContextIndexes([]);
      setIsExpanded(false);
    } finally {
      setIsSubmitting(false);
    }
  };

  if (!isExpanded) {
    return (
      <button
        type="button"
        onClick={openPanel}
        className="flex w-full items-center justify-center gap-2 rounded-[8px] border border-dashed border-border bg-secondary/25 py-3.5 text-sm text-muted-foreground transition-colors hover:border-primary/25 hover:bg-secondary/40 hover:text-foreground"
      >
        <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
        </svg>
        添加 Task
      </button>
    );
  }

  return (
    <div className="rounded-[8px] border border-border bg-secondary/35 p-4">
      <div className="mb-3 flex items-center justify-between">
        <span className="text-sm font-medium">新建 Task</span>
        <Button onClick={closePanel} size="sm" variant="secondary">
          取消
        </Button>
      </div>

      <div className="space-y-3">
        <Field label="Task 标题">
          <TextInput
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="要交给 Agent 的具体动作"
            autoFocus
          />
        </Field>

        <Field label="Workspace">
          <Select
            value={workspaceId}
            onChange={(event) => setWorkspaceId(event.target.value)}
          >
            <option value="">使用 Project 默认</option>
            {workspaces.map((workspace) => (
              <option key={workspace.id} value={workspace.id}>
                {workspace.name}
              </option>
            ))}
          </Select>
        </Field>

        <Field label="描述">
          <Textarea
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            rows={2}
            placeholder="补充验收口径、边界或实现提示"
            className="min-h-20"
          />
        </Field>

        <AgentBindingFields
          value={agentBinding}
          projectConfig={projectConfig}
          onChange={setAgentBinding}
        />

        {availableContexts.length > 0 && (
          <div className="rounded-[8px] border border-border bg-background p-3.5">
            <div className="mb-2 flex items-center justify-between gap-2">
              <div>
                <p className="text-xs font-medium text-muted-foreground">关联 Story 上下文</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  勾选后会把这些上下文源分配给 Task Agent，并在执行时由后端解析注入。
                </p>
              </div>
              <span className="rounded-[8px] border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground">
                已选 {selectedContextIndexes.length} / {availableContexts.length}
              </span>
            </div>

            <div className="mb-2 flex items-center gap-1">
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() =>
                  setSelectedContextIndexes(availableContexts.map((_, idx) => idx))
                }
                disabled={selectedContextIndexes.length === availableContexts.length}
              >
                全选
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() =>
                  setSelectedContextIndexes((current) =>
                    availableContexts
                      .map((_, idx) => idx)
                      .filter((idx) => !current.includes(idx)),
                  )
                }
                disabled={availableContexts.length === 0}
              >
                反选
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => setSelectedContextIndexes([])}
                disabled={selectedContextIndexes.length === 0}
              >
                清空
              </Button>
            </div>

            <div className="space-y-2">
              {availableContexts.map((context, index) => {
                const checked = selectedContextIndexes.includes(index);
                const meta = sourceKindMeta(context.kind);
                return (
                  <label
                    key={`${context.label ?? "context"}-${index}`}
                    className={`flex cursor-pointer items-start gap-3 rounded-[8px] border px-3 py-2 transition-colors ${
                      checked
                        ? "border-primary/40 bg-primary/5"
                        : "border-border bg-secondary/20 hover:bg-secondary/35"
                    }`}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggleContextSelection(index)}
                      className="mt-1 h-4 w-4 rounded border-border"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <Tooltip content={`${meta.icon} · ${meta.label}`} side="top">
                          <span
                            className={`rounded-[6px] border border-current/20 px-1.5 py-0.5 text-[10px] font-medium ${meta.color}`}
                          >
                            {meta.icon} {meta.label}
                          </span>
                        </Tooltip>
                        <span className="text-sm font-medium text-foreground">
                          {context.label?.trim() || `上下文 ${index + 1}`}
                        </span>
                      </div>
                      <p className="mt-1 truncate text-xs leading-5 text-muted-foreground">
                        {context.locator}
                      </p>
                    </div>
                  </label>
                );
              })}
            </div>
          </div>
        )}

        <div className="flex items-center justify-between">
          {formMessage || error ? (
            <p className="text-xs text-destructive">{formMessage || error}</p>
          ) : (
            <div />
          )}
          <Button
            onClick={() => void handleSubmit()}
            disabled={isSubmitting || !title.trim()}
            size="sm"
            variant="primary"
          >
            {isSubmitting ? "创建中..." : "创建"}
          </Button>
        </div>
      </div>
    </div>
  );
}
