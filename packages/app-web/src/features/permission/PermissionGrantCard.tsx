/**
 * PermissionGrantCard — Agent 权限申请审批卡片。
 *
 * 展示申请的 capability paths、理由、scope、状态，
 * 提供 approve/reject 操作按钮（仅 pending_user_approval 状态可用）。
 */

import { useState } from "react";
import type { PermissionGrant } from "../../types/permission";
import { isGrantPendingAction, isGrantActive } from "../../types/permission";
import {
  approvePermissionGrant,
  rejectPermissionGrant,
  revokePermissionGrant,
} from "../../services/permission";

export interface PermissionGrantCardProps {
  grant: PermissionGrant;
  onUpdated?: (updated: PermissionGrant) => void;
}

export function PermissionGrantCard({ grant, onUpdated }: PermissionGrantCardProps) {
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [localGrant, setLocalGrant] = useState(grant);

  const pending = isGrantPendingAction(localGrant.status);
  const active = isGrantActive(localGrant.status);

  const handleAction = async (action: "approve" | "reject" | "revoke") => {
    setIsSubmitting(true);
    setError(null);
    try {
      let result: PermissionGrant;
      switch (action) {
        case "approve":
          result = await approvePermissionGrant(localGrant.id);
          break;
        case "reject":
          result = await rejectPermissionGrant(localGrant.id);
          break;
        case "revoke":
          result = await revokePermissionGrant(localGrant.id);
          break;
      }
      setLocalGrant(result);
      onUpdated?.(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : "操作失败");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="permission-grant-card" data-status={localGrant.status}>
      <div className="permission-grant-card__header">
        <span className="permission-grant-card__badge">{statusLabel(localGrant.status)}</span>
        <span className="permission-grant-card__scope">{scopeLabel(localGrant.grant_scope)}</span>
        {localGrant.expires_at && (
          <span className="permission-grant-card__ttl">
            过期: {new Date(localGrant.expires_at).toLocaleString()}
          </span>
        )}
      </div>

      <div className="permission-grant-card__body">
        <p className="permission-grant-card__reason">{localGrant.reason}</p>
        <ul className="permission-grant-card__paths">
          {localGrant.requested_paths.map((p) => (
            <li key={p} className="permission-grant-card__path-item">
              <code>{p}</code>
            </li>
          ))}
        </ul>
        {localGrant.scope_escalation_intent && (
          <p className="permission-grant-card__escalation">
            Scope 升级目标: {localGrant.scope_escalation_intent.target_subject_kind}
          </p>
        )}
      </div>

      {error && <p className="permission-grant-card__error">{error}</p>}

      <div className="permission-grant-card__actions">
        {pending && (
          <>
            <button
              className="permission-grant-card__btn permission-grant-card__btn--approve"
              onClick={() => handleAction("approve")}
              disabled={isSubmitting}
            >
              批准
            </button>
            <button
              className="permission-grant-card__btn permission-grant-card__btn--reject"
              onClick={() => handleAction("reject")}
              disabled={isSubmitting}
            >
              拒绝
            </button>
          </>
        )}
        {active && (
          <button
            className="permission-grant-card__btn permission-grant-card__btn--revoke"
            onClick={() => handleAction("revoke")}
            disabled={isSubmitting}
          >
            撤销
          </button>
        )}
      </div>
    </div>
  );
}

function statusLabel(status: string): string {
  const map: Record<string, string> = {
    created: "创建",
    pending_policy: "评估中",
    pending_user_approval: "待审批",
    approved: "已批准",
    rejected: "已拒绝",
    applied: "生效中",
    failed: "失败",
    expired: "已过期",
    revoked: "已撤销",
    scope_escalated: "已升级",
  };
  return map[status] ?? status;
}

function scopeLabel(scope: string): string {
  const map: Record<string, string> = {
    turn: "单轮",
    session: "会话",
    workflow_step: "步骤",
  };
  return map[scope] ?? scope;
}
