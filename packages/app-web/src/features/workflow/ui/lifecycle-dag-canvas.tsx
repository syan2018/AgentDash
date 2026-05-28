/**
 * LifecycleDagCanvas —— 纯 DAG 画布（ReactFlow）的可复用封装。
 *
 * 受控组件：
 * - 输入：activities / transitions / entry_activity_key + 可选 workflowDefs（供节点 label 渲染）
 * - 输出：onActivitiesChange / onEdgesChange（整体替换），以及 onSelectActivity
 * - 不读写 store；所有副作用（位置持久化、布局计算）仍由画布自身管理
 */

import { useCallback, useEffect, useMemo, useRef } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  Panel,
  useNodesState,
  useEdgesState,
  BackgroundVariant,
  MarkerType,
  type Node,
  type Edge,
  type Connection,
  type OnConnect,
  type NodeMouseHandler,
  ReactFlowProvider,
  useReactFlow,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import type {
  ActivityDefinition,
  ActivityTransition,
  TransitionCondition,
  ValidationIssue,
  WorkflowDefinition,
} from "../../../types";
import { ADD_NEW_INPUT_HANDLE, DagNode, type DagNodeData } from "./dag-node";
import { applyDagreLayout, generateLinearEdges } from "../model/dag-layout";
import { syncLifecycleStepPortsForArtifactEdges } from "../model/lifecycle-port-sync";
import { transitionId as deriveTransitionId } from "../../../stores/workflowStore";

// ─── 常量 ───

const NODE_TYPES = { dagNode: DagNode };
const POSITION_STORAGE_PREFIX = "agentdash:dag-positions:";

// ─── 位置持久化 ───

function loadPositions(lifecycleKey: string): Record<string, { x: number; y: number }> {
  try {
    const raw = localStorage.getItem(POSITION_STORAGE_PREFIX + lifecycleKey);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

function savePositions(lifecycleKey: string, positions: Record<string, { x: number; y: number }>) {
  try {
    localStorage.setItem(POSITION_STORAGE_PREFIX + lifecycleKey, JSON.stringify(positions));
  } catch {
    // 忽略 quota 满
  }
}

// ─── 数据转换：domain ↔ ReactFlow ───

function buildActivityTooltip(step: ActivityDefinition, workflowName: string | null): string {
  const lines: string[] = [];
  if (step.description) lines.push(step.description);
  if (step.executor.kind === "agent") {
    lines.push(`Agent · ${workflowName ?? step.executor.workflow_key} · ${step.executor.session_policy}`);
  } else if (step.executor.kind === "human") {
    lines.push(`Human · ${step.executor.title ?? step.executor.form_schema_key}`);
  } else {
    lines.push(`Function · ${step.executor.type}`);
  }
  lines.push(`completion_policy: ${step.completion_policy.kind}`);
  const max = step.iteration_policy.max_attempts;
  lines.push(`iteration: ${max == null ? "∞" : max} · ${step.iteration_policy.artifact_alias}`);
  const join = typeof step.join_policy === "string" ? step.join_policy : `n_of_m(${step.join_policy.n_of_m.n})`;
  lines.push(`join: ${join}`);
  return lines.join("\n");
}

function countValidationIssuesForActivity(issues: ValidationIssue[], activityKey: string): number {
  return issues.filter(
    (i) =>
      i.severity === "error" &&
      (i.field_path === activityKey ||
        i.field_path.startsWith(`activities[${activityKey}]`) ||
        i.field_path.includes(`activity:${activityKey}`)),
  ).length;
}

function stepsToNodes(
  steps: ActivityDefinition[],
  entryStepKey: string,
  workflowDefs: WorkflowDefinition[],
  positions: Record<string, { x: number; y: number }>,
  validationIssues: ValidationIssue[],
): Node<DagNodeData>[] {
  const wfMap = new Map(workflowDefs.map((d) => [d.key, d]));
  return steps.map((step, idx) => {
    const workflowKey = step.executor.kind === "agent" ? step.executor.workflow_key : null;
    const wf = workflowKey ? wfMap.get(workflowKey) : null;
    const data: DagNodeData = {
      activityKey: step.key,
      description: step.description,
      executorKind: step.executor.kind,
      completionPolicyKind: step.completion_policy.kind,
      isEntryNode: step.key === entryStepKey,
      validationCount: countValidationIssuesForActivity(validationIssues, step.key),
      inputPorts: step.input_ports,
      outputPorts: step.output_ports,
      tooltip: buildActivityTooltip(step, wf?.name ?? null),
    };
    return {
      id: step.key,
      type: "dagNode",
      position: positions[step.key] ?? { x: idx * 300, y: 0 },
      data,
    };
  });
}

/** 与 store.transitionId 一致的派生：`${from}-->${to}#${idx}` */
function lifecycleEdgeId(e: ActivityTransition, idx: number): string {
  return deriveTransitionId(e, idx);
}

function edgeStrokeForCondition(condition: TransitionCondition, isFlow: boolean): string {
  if (condition.kind === "human_decision_equals") return "hsl(217 91% 60%)"; // blue-500
  if (isFlow) return "hsl(var(--primary))";
  return "hsl(var(--border))";
}

function buildEdgeLabel(e: ActivityTransition): string | undefined {
  const parts: string[] = [];
  if (e.kind === "flow") {
    const cond = conditionLabel(e.condition);
    if (cond) parts.push(cond);
  } else {
    const summary = e.artifact_bindings
      .slice(0, 2)
      .map((b) => `${b.from_port}→${b.to_port}`)
      .join(", ");
    if (summary) parts.push(summary);
    if (e.artifact_bindings.length > 2) parts.push(`+${e.artifact_bindings.length - 2}`);
  }
  if (e.max_traversals != null && e.max_traversals > 1) {
    parts.push(`↻${e.max_traversals}`);
  }
  return parts.length > 0 ? parts.join(" · ") : undefined;
}

function lifecycleEdgesToRfEdges(edges: ActivityTransition[], selectedTransitionId: string | null): Edge[] {
  return edges.map((e, idx) => {
    const isFlow = e.kind === "flow";
    const binding = e.artifact_bindings[0];
    const id = lifecycleEdgeId(e, idx);
    const isSelected = selectedTransitionId === id;
    const stroke = edgeStrokeForCondition(e.condition, isFlow);
    return {
      id,
      source: e.from,
      sourceHandle: isFlow ? undefined : binding?.from_port ?? undefined,
      target: e.to,
      targetHandle: isFlow ? undefined : binding?.to_port ?? undefined,
      markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16, color: stroke },
      label: buildEdgeLabel(e),
      labelStyle: { fontSize: 10, fill: "hsl(var(--muted-foreground))" },
      labelBgStyle: { fill: "hsl(var(--background))", fillOpacity: 0.9 },
      style: {
        stroke,
        strokeWidth: isSelected ? 3 : 2,
        strokeDasharray: isFlow ? undefined : "6 4",
      },
      selected: isSelected,
    };
  });
}

function rfEdgesToLifecycleEdges(rfEdges: Edge[]): ActivityTransition[] {
  return rfEdges.map((e) => {
    if (e.sourceHandle && e.targetHandle) {
      return {
        kind: "artifact" as const,
        from: e.source,
        to: e.target,
        condition: { kind: "always" as const },
        artifact_bindings: [{
          from_activity: e.source,
          from_port: e.sourceHandle,
          to_port: e.targetHandle,
          alias: "latest" as const,
        }],
      };
    }
    return {
      kind: "flow" as const,
      from: e.source,
      to: e.target,
      condition: { kind: "always" as const },
      artifact_bindings: [],
    };
  });
}

function conditionLabel(condition: ActivityTransition["condition"]): string | undefined {
  if (condition.kind === "always") return undefined;
  if (condition.kind === "human_decision_equals") {
    return `${condition.decision_port} = ${condition.value}`;
  }
  if (condition.kind === "artifact_field_equals") {
    return `${condition.port}.${condition.path} = ${String(condition.value)}`;
  }
  return `${condition.signal_key} = ${String(condition.value)}`;
}

// ─── Props ───

export interface LifecycleDagCanvasProps {
  /** 用作 position localStorage 桶的 key；建议用 lifecycle_key，新建用 "__new__"。 */
  storageKey: string;
  activities: ActivityDefinition[];
  transitions: ActivityTransition[];
  entryActivityKey: string;
  workflowDefs: WorkflowDefinition[];
  selectedActivityKey: string | null;
  /** 当前选中的 transition id（与 selectedActivityKey 互斥，由 shell 的 selection 模型派生） */
  selectedTransitionId?: string | null;
  onSelectActivity: (activityKey: string | null) => void;
  /** 选中边回调；shell 调 store.selectLifecycleTransition。 */
  onSelectTransition?: (transitionId: string | null) => void;
  onActivitiesChange: (next: ActivityDefinition[]) => void;
  onEdgesChange: (next: ActivityTransition[]) => void;
  /** Validation 结果，供节点角标 badge 渲染 */
  validationIssues?: ValidationIssue[];
  /** 顶部工具栏插槽（校验/保存按钮等）。 */
  toolbarExtras?: React.ReactNode;
  /** 右上状态插槽。 */
  statusExtras?: React.ReactNode;
  /** 底部叠加面板（例如 ValidationPanel）。 */
  bottomLeftOverlay?: React.ReactNode;
  /** 新增 activity 触发：由外层完成 key 生成和 layout 升级语义。 */
  onAddActivity?: () => void;
}

// ─── 内部 inner（需要在 ReactFlowProvider 内） ───

function LifecycleDagCanvasInner({
  storageKey,
  activities,
  transitions,
  entryActivityKey,
  workflowDefs,
  selectedActivityKey,
  selectedTransitionId = null,
  onSelectActivity,
  onSelectTransition,
  onActivitiesChange,
  onEdgesChange,
  validationIssues = [],
  toolbarExtras,
  statusExtras,
  bottomLeftOverlay,
  onAddActivity,
}: LifecycleDagCanvasProps) {
  const reactFlowInstance = useReactFlow();
  const positions = useRef(loadPositions(storageKey));

  const [nodes, setNodes, onNodesChange] = useNodesState<Node<DagNodeData>>([]);
  const [rfEdges, setRfEdges, onRfEdgesChange] = useEdgesState<Edge>([]);

  // ── 从 props 同步到 RF state ──
  // 每次 props 变化都通过 stepsToNodes / lifecycleEdgesToRfEdges 重建节点；
  // positions cache 保留拖动后的坐标，validationIssues 与 selectedTransitionId
  // 也一并参与 deps 以驱动徽章和高亮刷新。
  useEffect(() => {
    setNodes(stepsToNodes(activities, entryActivityKey, workflowDefs, positions.current, validationIssues));
    setRfEdges(lifecycleEdgesToRfEdges(transitions, selectedTransitionId));
  }, [
    activities,
    transitions,
    entryActivityKey,
    workflowDefs,
    validationIssues,
    selectedTransitionId,
    setNodes,
    setRfEdges,
  ]);

  const handleNodesChangeWrapped = useCallback(
    (changes: Parameters<typeof onNodesChange>[0]) => {
      const nonRemove = changes.filter((c) => c.type !== "remove");
      if (nonRemove.length > 0) onNodesChange(nonRemove);
      if (changes.some((c) => c.type === "position")) {
        requestAnimationFrame(() => {
          const pos: Record<string, { x: number; y: number }> = {};
          for (const n of reactFlowInstance.getNodes()) pos[n.id] = n.position;
          positions.current = pos;
          savePositions(storageKey, pos);
        });
      }
    },
    [onNodesChange, reactFlowInstance, storageKey],
  );

  const handleEdgesChangeWrapped = useCallback(
    (changes: Parameters<typeof onRfEdgesChange>[0]) => {
      const nonRemove = changes.filter((c) => c.type !== "remove");
      if (nonRemove.length > 0) onRfEdgesChange(nonRemove);
    },
    [onRfEdgesChange],
  );

  // ── Delete 键：统一回写 props ──
  const handleDelete = useCallback(
    ({ nodes: deletedNodes, edges: deletedEdges }: { nodes: Node[]; edges: Edge[] }) => {
      const deletedNodeKeys = new Set(deletedNodes.map((n) => n.id));
      let newSteps = activities;
      let newEdges = transitions;

      if (deletedNodeKeys.size > 0) {
        newSteps = activities.filter((s) => !deletedNodeKeys.has(s.key));
        newEdges = newEdges.filter(
          (e) => !deletedNodeKeys.has(e.from) && !deletedNodeKeys.has(e.to),
        );
        if (selectedActivityKey != null && deletedNodeKeys.has(selectedActivityKey)) {
          onSelectActivity(null);
        }
      }

      if (deletedEdges.length > 0) {
        const deletedEdgeIds = new Set(deletedEdges.map((e) => e.id));
        newEdges = newEdges.filter((e, idx) => !deletedEdgeIds.has(lifecycleEdgeId(e, idx)));
        if (selectedTransitionId && deletedEdgeIds.has(selectedTransitionId)) {
          onSelectTransition?.(null);
        }
      }

      if (newSteps !== activities) onActivitiesChange(newSteps);
      if (newEdges !== transitions) onEdgesChange(newEdges);
    },
    [
      activities,
      transitions,
      selectedActivityKey,
      selectedTransitionId,
      onSelectActivity,
      onSelectTransition,
      onActivitiesChange,
      onEdgesChange,
    ],
  );

  // ── Connect：新边 ──
  const handleConnect: OnConnect = useCallback(
    (connection: Connection) => {
      if (!connection.source || !connection.target) return;
      // 允许自连与回环：lifecycle 支持 human_decision/重试/回评等需要环的语义，
      // 由运行时 transition.max_traversals + iteration_policy.max_attempts 兜底；
      // validator 会对未设阈值的环发软警告（参见 dag-layout.findUnboundedCycles）。

      const isAddNewInputDrop = connection.targetHandle === ADD_NEW_INPUT_HANDLE;
      const hasSourcePort = !!connection.sourceHandle && connection.sourceHandle !== "__default_out";
      const hasTargetPort =
        !!connection.targetHandle &&
        connection.targetHandle !== "__default_in" &&
        !isAddNewInputDrop;
      const isArtifactConnect = hasSourcePort || hasTargetPort || isAddNewInputDrop;

      if (isArtifactConnect) {
        const fromPort = connection.sourceHandle ?? "__default_out";
        let toPort = isAddNewInputDrop ? "__default_in" : (connection.targetHandle ?? "__default_in");

        let newSteps = activities;
        // 命中现有 input handle 时检查重复 binding（ghost 和 default 走下面的自动建端口分支）
        if (
          !isAddNewInputDrop &&
          toPort !== "__default_in" &&
          transitions.some(
            (e) =>
              e.kind === "artifact" &&
              e.to === connection.target &&
              e.artifact_bindings.some((binding) => binding.to_port === toPort),
          )
        ) {
          return;
        }
        if (toPort === "__default_in") {
          const targetIdx = activities.findIndex((s) => s.key === connection.target);
          if (targetIdx >= 0) {
            const targetStep = activities[targetIdx];
            // 决定新 input port 的 key：
            // - source 是真实端口 → 复用同名 key（默认 handle 自动建端口的旧逻辑保留）
            // - source 是 __default_out 或 ghost 落点 → 取一个不冲突的 in_N
            let newKey: string;
            if (fromPort !== "__default_out" && !targetStep.input_ports.some((p) => p.key === fromPort)) {
              newKey = fromPort;
            } else if (fromPort !== "__default_out" && !isAddNewInputDrop) {
              // 真实 source + 默认 handle，端口已存在 → 复用
              newKey = fromPort;
            } else {
              const existing = new Set(targetStep.input_ports.map((p) => p.key));
              let n = targetStep.input_ports.length + 1;
              while (existing.has(`in_${n}`)) n += 1;
              newKey = `in_${n}`;
            }
            const hasPort = targetStep.input_ports.some((p) => p.key === newKey);
            if (!hasPort) {
              newSteps = activities.map((s, i) =>
                i === targetIdx
                  ? {
                      ...s,
                      input_ports: [
                        ...s.input_ports,
                        {
                          key: newKey,
                          description: "",
                          context_strategy: "full" as const,
                          standalone_fulfillment: "required" as const,
                        },
                      ],
                    }
                  : s,
              );
            }
            toPort = newKey;
          }
        }

        const newEdge: ActivityTransition = {
          kind: "artifact",
          from: connection.source,
          to: connection.target,
          condition: { kind: "always" },
          artifact_bindings: [{
            from_activity: connection.source,
            from_port: fromPort,
            to_port: toPort,
            alias: "latest",
          }],
        };
        const nextEdges = [...transitions, newEdge];
        const synced = syncLifecycleStepPortsForArtifactEdges({
          steps: newSteps,
          edges: nextEdges,
          workflows: workflowDefs,
        });
        if (synced.changed || newSteps !== activities) onActivitiesChange(synced.steps);
        onEdgesChange(nextEdges);
      } else {
        const existsFlow = transitions.some(
          (e) =>
            e.kind === "flow" &&
            e.from === connection.source &&
            e.to === connection.target,
        );
        if (existsFlow) return;
        const newEdge: ActivityTransition = {
          kind: "flow",
          from: connection.source,
          to: connection.target,
          condition: { kind: "always" },
          artifact_bindings: [],
        };
        onEdgesChange([...transitions, newEdge]);
      }
    },
    [transitions, activities, workflowDefs, onActivitiesChange, onEdgesChange],
  );

  const handleNodeClick: NodeMouseHandler = useCallback(
    (_e, node) => {
      onSelectActivity(node.id);
    },
    [onSelectActivity],
  );

  const handleEdgeClick = useCallback(
    (_e: React.MouseEvent, edge: Edge) => {
      onSelectTransition?.(edge.id);
    },
    [onSelectTransition],
  );

  const handlePaneClick = useCallback(() => {
    onSelectActivity(null);
    onSelectTransition?.(null);
  }, [onSelectActivity, onSelectTransition]);

  const handleAutoLayout = useCallback(() => {
    const freshNodes = stepsToNodes(activities, entryActivityKey, workflowDefs, positions.current, validationIssues);
    const freshEdges = lifecycleEdgesToRfEdges(transitions, selectedTransitionId);
    const laid = applyDagreLayout(freshNodes, freshEdges);
    setNodes(laid);
    const pos: Record<string, { x: number; y: number }> = {};
    for (const n of laid) pos[n.id] = n.position;
    positions.current = pos;
    savePositions(storageKey, pos);
    requestAnimationFrame(() => reactFlowInstance.fitView({ padding: 0.2 }));
  }, [
    activities,
    transitions,
    entryActivityKey,
    workflowDefs,
    validationIssues,
    selectedTransitionId,
    setNodes,
    reactFlowInstance,
    storageKey,
  ]);

  const handleAutoWire = useCallback(() => {
    const nodeIds = activities.map((s) => s.key);
    const getFirstOutputPort = (nodeId: string): string | null => {
      const step = activities.find((s) => s.key === nodeId);
      if (!step) return null;
      if (step.output_ports.length > 0 && step.output_ports[0].key) return step.output_ports[0].key;
      return "__default_out";
    };
    const getFirstInputPort = (nodeId: string): string | null => {
      const step = activities.find((s) => s.key === nodeId);
      if (!step) return null;
      if (step.input_ports.length > 0 && step.input_ports[0].key) return step.input_ports[0].key;
      return "__default_in";
    };
    const linearEdges = generateLinearEdges(nodeIds, getFirstOutputPort, getFirstInputPort);
    onEdgesChange(rfEdgesToLifecycleEdges(linearEdges));
  }, [activities, onEdgesChange]);

  // ── 首次挂载自动 layout（所有节点 ≈ (0,0)）──
  const hasAutoLayouted = useRef(false);
  useEffect(() => {
    if (hasAutoLayouted.current || nodes.length === 0) return;
    const allAtOrigin = nodes.every(
      (n) => Math.abs(n.position.x) < 10 && Math.abs(n.position.y) < 10,
    );
    if (allAtOrigin && nodes.length > 1) {
      hasAutoLayouted.current = true;
      requestAnimationFrame(handleAutoLayout);
    }
  }, [nodes, handleAutoLayout]);

  const selectedIds = useMemo(
    () => (selectedActivityKey ? new Set([selectedActivityKey]) : new Set<string>()),
    [selectedActivityKey],
  );
  const nodesForRender = useMemo(
    () =>
      nodes.map((n) =>
        selectedIds.has(n.id) ? { ...n, selected: true } : n.selected ? { ...n, selected: false } : n,
      ),
    [nodes, selectedIds],
  );

  return (
    <ReactFlow
      nodes={nodesForRender}
      edges={rfEdges}
      onNodesChange={handleNodesChangeWrapped}
      onEdgesChange={handleEdgesChangeWrapped}
      onConnect={handleConnect}
      onNodeClick={handleNodeClick}
      onEdgeClick={handleEdgeClick}
      onPaneClick={handlePaneClick}
      onDelete={handleDelete}
      nodeTypes={NODE_TYPES}
      fitView
      fitViewOptions={{ padding: 0.2 }}
      deleteKeyCode="Delete"
      proOptions={{ hideAttribution: true }}
    >
      <Background variant={BackgroundVariant.Dots} gap={20} size={1} color="hsl(var(--border))" />
      <MiniMap
        nodeStrokeWidth={2}
        style={{ border: "1px solid hsl(var(--border))", borderRadius: 8 }}
      />
      <Controls
        showInteractive={false}
        style={{ border: "1px solid hsl(var(--border))", borderRadius: 8 }}
      />

      <Panel position="top-left">
        <div className="flex items-center gap-2 rounded-[8px] border border-border bg-background/95 px-3 py-2 shadow-sm backdrop-blur-sm">
          {onAddActivity && (
            <button
              type="button"
              onClick={onAddActivity}
              className="agentdash-button-secondary px-2 py-1 text-xs"
            >
              + 添加节点
            </button>
          )}
          <button
            type="button"
            onClick={handleAutoLayout}
            className="agentdash-button-secondary px-2 py-1 text-xs"
          >
            自动布局
          </button>
          <button
            type="button"
            onClick={handleAutoWire}
            className="agentdash-button-secondary px-2 py-1 text-xs"
            title="按 steps 顺序自动生成线性连线"
          >
            线性连线
          </button>
          {toolbarExtras && (
            <>
              <div className="mx-1 h-5 w-px bg-border" />
              {toolbarExtras}
            </>
          )}
        </div>
      </Panel>

      {statusExtras && <Panel position="top-right">{statusExtras}</Panel>}
      {bottomLeftOverlay && <Panel position="bottom-left">{bottomLeftOverlay}</Panel>}
    </ReactFlow>
  );
}

// ─── 导出封装 ───

export function LifecycleDagCanvas(props: LifecycleDagCanvasProps) {
  return (
    <ReactFlowProvider>
      <LifecycleDagCanvasInner {...props} />
    </ReactFlowProvider>
  );
}
