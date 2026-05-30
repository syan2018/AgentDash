/** Agent Permission System — 前端类型定义 */

export type GrantScope = "turn" | "session" | "workflow_step";

export type GrantStatus =
  | "created"
  | "pending_policy"
  | "pending_user_approval"
  | "approved"
  | "rejected"
  | "applied"
  | "failed"
  | "expired"
  | "revoked"
  | "scope_escalated";

export interface ScopeEscalationIntent {
  target_subject_kind: string;
  unlocked_paths: string[];
}

export interface PolicyDecision {
  outcome: "auto_approved" | "needs_user_approval" | "rejected";
  matched_rules: string[];
  reason: string;
}

export interface PermissionGrant {
  id: string;
  run_id: string;
  session_id: string;
  requested_paths: string[];
  reason: string;
  grant_scope: GrantScope;
  expires_at: string | null;
  scope_escalation_intent: ScopeEscalationIntent | null;
  status: GrantStatus;
  policy_decision: PolicyDecision | null;
  approved_by: string | null;
  created_at: string;
  updated_at: string;
}

export function isGrantActive(status: GrantStatus): boolean {
  return status === "applied" || status === "scope_escalated";
}

export function isGrantPendingAction(status: GrantStatus): boolean {
  return status === "pending_user_approval";
}
