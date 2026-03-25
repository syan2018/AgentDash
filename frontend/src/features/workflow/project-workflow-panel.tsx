/**
 * ProjectWorkflowPanel — 项目设置页内嵌的 Workflow Assignment 管理
 *
 * 只负责「为当前项目的各角色分配/取消 lifecycle」的轻量操作。
 * 完整的定义管理请使用 /dashboard/workflow。
 */

import { useEffect, useMemo, useState } from "react";

import type { WorkflowAgentRole, WorkflowAssignment } from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import { ROLE_LABEL, ROLE_ORDER } from "./shared-labels";

const EMPTY_ASSIGNMENTS: WorkflowAssignment[] = [];

export function ProjectWorkflowPanel({ projectId }: { projectId: string }) {
  const lifecycles = useWorkflowStore((s) => s.lifecycleDefinitions);
  const assignments = useWorkflowStore(
    (s) => s.assignmentsByProjectId[projectId] ?? EMPTY_ASSIGNMENTS,
  );
  const fetchLifecycles = useWorkflowStore((s) => s.fetchLifecycles);
  const fetchProjectAssignments = useWorkflowStore((s) => s.fetchProjectAssignments);
  const assignLifecycleToProject = useWorkflowStore((s) => s.assignLifecycleToProject);
  const error = useWorkflowStore((s) => s.error);

  const [busyRole, setBusyRole] = useState<WorkflowAgentRole | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    void fetchLifecycles();
    void fetchProjectAssignments(projectId);
  }, [projectId, fetchLifecycles, fetchProjectAssignments]);

  useEffect(() => {
    if (!message) return;
    const t = setTimeout(() => setMessage(null), 3000);
    return () => clearTimeout(t);
  }, [message]);

  const assignmentByRole = useMemo(() => {
    const map: Partial<Record<WorkflowAgentRole, WorkflowAssignment>> = {};
    for (const a of assignments) {
      if (a.enabled && (!map[a.role] || a.is_default)) {
        map[a.role] = a;
      }
    }
    return map;
  }, [assignments]);

  const handleAssign = async (role: WorkflowAgentRole, lifecycleId: string) => {
    if (!lifecycleId) return;
    setBusyRole(role);
    const result = await assignLifecycleToProject({
      project_id: projectId,
      lifecycle_id: lifecycleId,
      role,
      enabled: true,
      is_default: true,
    });
    if (result) {
      const lc = lifecycles.find((l) => l.id === lifecycleId);
      setMessage(`已设为 ${ROLE_LABEL[role]} 默认：${lc?.name ?? lifecycleId}`);
    }
    setBusyRole(null);
  };

  return (
    <div className="space-y-3">
      {message && (
        <p className="rounded-[8px] border border-emerald-300/30 bg-emerald-500/5 px-3 py-2 text-xs text-emerald-600">
          {message}
        </p>
      )}
      {error && (
        <p className="rounded-[8px] border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          {error}
        </p>
      )}

      <div className="rounded-[12px] border border-border overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-border bg-secondary/35 text-left text-[11px] uppercase tracking-[0.1em] text-muted-foreground">
              <th className="px-3 py-2 font-medium">角色</th>
              <th className="px-3 py-2 font-medium">当前 Lifecycle</th>
              <th className="px-3 py-2 font-medium">操作</th>
            </tr>
          </thead>
          <tbody>
            {ROLE_ORDER.map((role) => {
              const current = assignmentByRole[role];
              const currentLc = current ? lifecycles.find((l) => l.id === current.lifecycle_id) : null;
              const isBusy = busyRole === role;

              return (
                <tr key={role} className="border-b border-border last:border-b-0">
                  <td className="px-3 py-2.5 text-xs font-medium text-foreground">{ROLE_LABEL[role]}</td>
                  <td className="px-3 py-2.5 text-xs text-muted-foreground">
                    {currentLc ? currentLc.name : <span className="italic">未配置</span>}
                  </td>
                  <td className="px-3 py-2.5">
                    <select
                      value={current?.lifecycle_id ?? ""}
                      onChange={(e) => void handleAssign(role, e.target.value)}
                      disabled={isBusy}
                      className="h-7 rounded-[8px] border border-border bg-background px-2 text-xs outline-none focus:border-primary/30"
                    >
                      <option value="">(无)</option>
                      {lifecycles
                        .filter((l) => l.status === "active")
                        .map((l) => (
                          <option key={l.id} value={l.id}>{l.name}</option>
                        ))}
                    </select>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
