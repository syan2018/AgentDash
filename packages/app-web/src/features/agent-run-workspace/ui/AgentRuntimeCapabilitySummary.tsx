import { Badge } from "@agentdash/ui";

import type { AgentRunRuntimeInspectResponse } from "../../../services/agentRunRuntime";

export interface AgentRuntimeCapabilitySummaryProps {
  inspect: AgentRunRuntimeInspectResponse | null;
}

export function AgentRuntimeCapabilitySummary({ inspect }: AgentRuntimeCapabilitySummaryProps) {
  const binding = inspect?.binding;
  const snapshot = inspect?.snapshot;
  if (!binding || !snapshot) return null;
  const strengths = [...new Set(binding.hook_plan.entries.map((entry) => entry.delivered_strength))];
  const provenance = binding.profile_provenance;
  return (
    <div
      className="inline-flex flex-wrap items-center gap-1.5"
      title={`service=${provenance.service_digest}\ntransport=${provenance.transport_digest}\nhost_policy=${provenance.host_policy_digest}`}
    >
      <Badge variant="neutral">{snapshot.bound_profile.reference_class}</Badge>
      <Badge variant="info">context {snapshot.bound_profile.context.fidelity}</Badge>
      {strengths.map((strength) => (
        <Badge key={strength} variant="neutral">hook {strength}</Badge>
      ))}
    </div>
  );
}
