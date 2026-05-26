import type { ProjectSessionEntry } from "../../types";

export type SessionParentRelationKind = NonNullable<
  ProjectSessionEntry["parent_relation_kind"]
>;

export interface SessionLinkedChild {
  session: ProjectSessionEntry;
  relation_kind: SessionParentRelationKind;
}

export function normalizeParentRelationKind(
  relationKind: ProjectSessionEntry["parent_relation_kind"] | undefined,
): SessionParentRelationKind {
  return relationKind ?? "companion";
}

export function sessionParentRelationLabel(
  relationKind: ProjectSessionEntry["parent_relation_kind"] | undefined,
): string {
  switch (normalizeParentRelationKind(relationKind)) {
    case "fork":
      return "fork";
    case "rollback_branch":
      return "rollback";
    case "spawned_agent":
      return "subagent";
    case "companion":
      return "companion";
  }
  return "companion";
}
