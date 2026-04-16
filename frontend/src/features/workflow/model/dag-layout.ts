import dagre from "@dagrejs/dagre";
import type { Node, Edge } from "@xyflow/react";

const NODE_WIDTH = 240;
const NODE_HEIGHT = 100;
const RANK_SEP = 100;
const NODE_SEP = 50;

/**
 * 使用 dagre 算法对 DAG 节点进行自动布局（从左到右）。
 * 返回带有新 position 的节点数组（不修改原数组）。
 */
export function applyDagreLayout(nodes: Node[], edges: Edge[]): Node[] {
  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: "LR", ranksep: RANK_SEP, nodesep: NODE_SEP });

  for (const node of nodes) {
    g.setNode(node.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  }
  for (const edge of edges) {
    g.setEdge(edge.source, edge.target);
  }

  dagre.layout(g);

  return nodes.map((node) => {
    const pos = g.node(node.id);
    return {
      ...node,
      position: { x: pos.x - NODE_WIDTH / 2, y: pos.y - NODE_HEIGHT / 2 },
    };
  });
}

/**
 * 按 steps 数组顺序生成线性 edges（前一个 node 的第一个 output port → 后一个 node 的第一个 input port）。
 * 如果某个 node 没有对应的 port，则使用 default handle 连接。
 */
export function generateLinearEdges(
  nodeIds: string[],
  getOutputPort: (nodeId: string) => string | null,
  getInputPort: (nodeId: string) => string | null,
): Edge[] {
  const edges: Edge[] = [];
  for (let i = 0; i < nodeIds.length - 1; i++) {
    const src = nodeIds[i];
    const tgt = nodeIds[i + 1];
    const srcPort = getOutputPort(src);
    const tgtPort = getInputPort(tgt);
    if (srcPort && tgtPort) {
      edges.push({
        id: `${src}:${srcPort}--${tgt}:${tgtPort}`,
        source: src,
        sourceHandle: srcPort,
        target: tgt,
        targetHandle: tgtPort,
      });
    }
  }
  return edges;
}

/**
 * 检测添加一条 source→target 边是否会产生环。
 * 接受任意含 { source, target } 或 { from_node, to_node } 的边数组。
 */
export function wouldCreateCycle(
  existingEdges: ReadonlyArray<{ source?: string; target?: string; from_node?: string; to_node?: string }>,
  newSource: string,
  newTarget: string,
): boolean {
  const adj = new Map<string, string[]>();
  for (const edge of existingEdges) {
    const s = edge.source ?? edge.from_node ?? "";
    const t = edge.target ?? edge.to_node ?? "";
    const list = adj.get(s) ?? [];
    list.push(t);
    adj.set(s, list);
  }
  const visited = new Set<string>();
  const stack = [newTarget];
  while (stack.length > 0) {
    const cur = stack.pop()!;
    if (cur === newSource) return true;
    if (visited.has(cur)) continue;
    visited.add(cur);
    for (const next of adj.get(cur) ?? []) {
      stack.push(next);
    }
  }
  return false;
}
