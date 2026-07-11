import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { AgentRunRuntimeInspectResponse } from "../../../services/agentRunRuntime";
import { AgentRuntimeCapabilitySummary } from "./AgentRuntimeCapabilitySummary";

describe("AgentRuntimeCapabilitySummary", () => {
  it("renders the bound reference class, context fidelity, provenance and hook strength", () => {
    const inspect: AgentRunRuntimeInspectResponse = {
      target: { run_id: "run-1", agent_id: "agent-1" },
      binding: {
        target: { run_id: "run-1", agent_id: "agent-1" },
        thread_id: "thread-1",
        binding_id: "binding-1",
        driver_generation: 1n,
        source_thread_id: "source-1",
        profile_digest: "sha256:profile",
        profile_provenance: {
          service_digest: "sha256:service",
          transport_digest: "sha256:transport",
          host_policy_digest: "sha256:host",
        },
        surface_digest: "sha256:surface",
        bound_profile: profile(),
        hook_plan: {
          revision: 1n,
          digest: "sha256:hooks",
          entries: [{
            definition_id: "hook-1",
            point: "before_tool",
            actions: ["observe"],
            delivered_strength: "exact_synchronous",
            failure_policy: "fail_closed",
            required: true,
            site: "driver_native",
          }],
        },
      },
      snapshot: {
        thread_id: "thread-1",
        revision: 1n,
        status: "active",
        active_turn_id: null,
        binding_id: "binding-1",
        profile_digest: "sha256:profile",
        bound_profile: profile(),
        active_checkpoint_id: null,
        context_revision: 1n,
        settings_revision: 1n,
        tool_set_revision: 1n,
        pending_interactions: [],
        command_availability: {},
        transcript: [],
        transcript_fidelity: "platform_exact",
      },
    };
    const html = renderToStaticMarkup(<AgentRuntimeCapabilitySummary inspect={inspect} />);
    expect(html).toContain("managed_thread");
    expect(html).toContain("platform_exact");
    expect(html).toContain("exact_synchronous");
    expect(html).toContain("sha256:transport");
  });
});

function profile(): NonNullable<AgentRunRuntimeInspectResponse["binding"]>["bound_profile"] {
  return {
    reference_class: "managed_thread",
    input: { modalities: ["text"] },
    instruction: { channels: ["system"], configuration_boundary: "thread_start" },
    tools: { channels: ["direct_callback"], configuration_boundary: "turn_start", cancellation: true },
    workspace: { capabilities: ["read"], mechanism: "host_adapted_exact" },
    interactions: { kinds: [], durable_correlation: true },
    lifecycle: ["thread_read", "turn_start"],
    hooks: { points: [], configuration_boundary: "thread_start" },
    context: { capabilities: ["read"], fidelity: "platform_exact", activation_idempotent: true },
    telemetry_config: ["deltas"],
  };
}
