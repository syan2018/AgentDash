import { describe, expect, it } from "vitest";

import {
  decodeManagedRuntimeOperationReceipt,
  decodeManagedRuntimeSnapshot,
  encodeManagedRuntimeOperationReceipt,
  encodeManagedRuntimeSnapshot,
} from "../../../generated/agent-runtime-validators";
import {
  decodeAgentChangePage,
  decodeAgentCommandEnvelope,
  decodeAgentServiceU64,
  decodeAgentSnapshot,
  decodeAgentToolInvocation,
  encodeAgentChangePage,
  encodeAgentCommandEnvelope,
  encodeAgentSnapshot,
  encodeAgentToolInvocation,
} from "../../../generated/agent-service-codecs";
import {
  decodeRuntimeWireEnvelope,
  encodeRuntimeWireEnvelope,
} from "../../../generated/agent-runtime-wire-codecs";

const MAX_U64 = "18446744073709551615";
const MAX_U64_BIGINT = 18_446_744_073_709_551_615n;

describe("Managed Runtime canonical u64 codecs", () => {
  it("round-trips the Product command receipt revision", () => {
    const receipt = decodeManagedRuntimeOperationReceipt({
      operation_id: "operation-1",
      thread_id: "thread-1",
      accepted_revision: MAX_U64,
      status: "accepted",
      evidence: null,
      duplicate: false,
    });

    expect(receipt.accepted_revision).toBe(MAX_U64_BIGINT);
    expect(encodeManagedRuntimeOperationReceipt(receipt).accepted_revision).toBe(
      MAX_U64,
    );
  });

  it.each([1, "01", "-1", "18446744073709551616"])(
    "rejects non-canonical Runtime root u64 %s",
    (value) => {
      expect(() =>
        decodeManagedRuntimeOperationReceipt({
          operation_id: "operation-1",
          thread_id: "thread-1",
          accepted_revision: value,
          status: "accepted",
          evidence: null,
          duplicate: false,
        }),
      ).toThrow("$.accepted_revision");
    },
  );

  it("recursively round-trips item presentation timestamps and sleep duration", () => {
    const decoded = decodeManagedRuntimeSnapshot({
      thread_id: "thread-1",
      revision: MAX_U64,
      latest_change_sequence: MAX_U64,
      captured_at_ms: MAX_U64,
      lifecycle: "active",
      active_turn_id: "turn-1",
      turns: [{ id: "turn-1", status: "completed", item_ids: ["item-1"] }],
      items: [{
        id: "item-1",
        turn_id: "turn-1",
        status: "completed",
        presentation: {
          body: { kind: "sleep", duration_ms: MAX_U64 },
          started_at_ms: MAX_U64,
          updated_at_ms: MAX_U64,
          terminal: {
            outcome: "completed",
            completed_at_ms: MAX_U64,
            duration_ms: MAX_U64,
            process_exit: null,
            error: null,
          },
          body_digest: "sha256:body",
          presentation_digest: "sha256:presentation",
        },
      }],
      interactions: [],
      thread_name: null,
      thread_name_source: null,
      operations: [],
      source_binding: null,
      authority: "source_authoritative",
      fidelity: "exact",
      command_availability: {},
    });

    expect(decoded.items[0]?.presentation).toMatchObject({
      started_at_ms: MAX_U64_BIGINT,
      updated_at_ms: MAX_U64_BIGINT,
      body: { duration_ms: MAX_U64_BIGINT },
      terminal: {
        completed_at_ms: MAX_U64_BIGINT,
        duration_ms: MAX_U64_BIGINT,
      },
    });
    expect(encodeManagedRuntimeSnapshot(decoded).items[0]?.presentation).toMatchObject({
      started_at_ms: MAX_U64,
      updated_at_ms: MAX_U64,
      body: { duration_ms: MAX_U64 },
      terminal: {
        completed_at_ms: MAX_U64,
        duration_ms: MAX_U64,
      },
    });
  });

  it.each([
    [
      "terminal outcome",
      { kind: "sleep", duration_ms: "1" },
      {
        outcome: "future_outcome",
        completed_at_ms: "1",
        duration_ms: "1",
        process_exit: null,
        error: null,
      },
      "terminal.outcome",
    ],
    [
      "content block",
      {
        kind: "agent_message",
        content: [{ kind: "future_content" }],
        phase: null,
      },
      null,
      "content[0].kind",
    ],
    [
      "plan step status",
      {
        kind: "plan",
        explanation: null,
        steps: [{ id: null, text: "step", status: "future_status" }],
      },
      null,
      "steps[0].status",
    ],
    [
      "file search mode",
      {
        kind: "file_search",
        mode: "future_mode",
        query: "query",
        path: null,
        matches: [],
      },
      null,
      "body.mode",
    ],
    [
      "file change kind",
      {
        kind: "file_change",
        changes: [{
          path: "file.txt",
          change_kind: "future_change",
          patch: "",
          moved_to: null,
        }],
        output: [],
      },
      null,
      "changes[0].change_kind",
    ],
    [
      "command output stream",
      {
        kind: "command_execution",
        command: "echo",
        cwd: null,
        output: [{ stream: "future_stream", text: "output" }],
      },
      null,
      "output[0].stream",
    ],
  ])(
    "rejects unknown nested presentation %s discriminant",
    (_family, body, terminal, expectedPath) => {
      const snapshot = {
        thread_id: "thread-1",
        revision: "1",
        latest_change_sequence: "1",
        captured_at_ms: "1",
        lifecycle: "active",
        active_turn_id: "turn-1",
        turns: [{
          id: "turn-1",
          status: terminal === null ? "running" : "completed",
          item_ids: ["item-1"],
        }],
        items: [{
          id: "item-1",
          turn_id: "turn-1",
          status: terminal === null ? "running" : "completed",
          presentation: {
            body,
            started_at_ms: "1",
            updated_at_ms: "1",
            terminal,
            body_digest: "sha256:body",
            presentation_digest: "sha256:presentation",
          },
        }],
        interactions: [],
        thread_name: null,
        thread_name_source: null,
        operations: [],
        source_binding: null,
        authority: "source_authoritative",
        fidelity: "exact",
        command_availability: {},
      };

      expect(() => decodeManagedRuntimeSnapshot(snapshot)).toThrow(expectedPath);

      const validWire = structuredClone(snapshot);
      validWire.items[0]!.presentation.body = {
        kind: "sleep",
        duration_ms: "1",
      };
      validWire.items[0]!.presentation.terminal = null;
      const semantic = decodeManagedRuntimeSnapshot(validWire);
      semantic.items[0]!.presentation.body = body as never;
      semantic.items[0]!.presentation.terminal = terminal as never;

      expect(() => encodeManagedRuntimeSnapshot(semantic)).toThrow(expectedPath);
    },
  );
});

describe("Complete Agent canonical u64 codecs", () => {
  it("round-trips command generations and expected revisions at u64 max", () => {
    const decoded = decodeAgentCommandEnvelope({
      meta: {
        command_id: "command-1",
        effect_id: "effect-1",
        idempotency_key: "idem-1",
        binding_generation: MAX_U64,
        expected_snapshot_revision: MAX_U64,
      },
      source: "source-1",
      command: { kind: "request_compaction" },
    });

    expect(decoded.meta.binding_generation).toBe(MAX_U64_BIGINT);
    expect(decoded.meta.expected_snapshot_revision).toBe(MAX_U64_BIGINT);
    expect(encodeAgentCommandEnvelope(decoded).meta).toMatchObject({
      binding_generation: MAX_U64,
      expected_snapshot_revision: MAX_U64,
    });
  });

  it("round-trips snapshot, source observation, applied surface, and change time", () => {
    const snapshot = decodeAgentSnapshot({
      source: "source-1",
      revision: MAX_U64,
      lifecycle: "active",
      active_turn_id: null,
      turns: [],
      interactions: [],
      thread_name: {
        thread_name: "thread",
        source_info: {
          authority: "agent_authoritative",
          source_revision: null,
          fidelity: "exact",
          observed_at_ms: MAX_U64,
        },
      },
      source_info: {
        authority: "agent_authoritative",
        source_revision: null,
        fidelity: "exact",
        observed_at_ms: MAX_U64,
      },
      applied_surface: {
        revision: MAX_U64,
        digest: "sha256:surface",
        contributions: [],
      },
      initial_context: null,
    });
    const page = decodeAgentChangePage({
      source: "source-1",
      changes: [
        {
          cursor: "cursor-1",
          source_revision: null,
          occurred_at_ms: MAX_U64,
          payload: {
            kind: "surface_applied",
            applied: encodeAgentSnapshot(snapshot).applied_surface,
          },
        },
      ],
      next: "cursor-1",
      gap: false,
    });

    expect(snapshot.revision).toBe(MAX_U64_BIGINT);
    expect(snapshot.thread_name?.source_info.observed_at_ms).toBe(MAX_U64_BIGINT);
    expect(page.changes[0]?.occurred_at_ms).toBe(MAX_U64_BIGINT);
    expect(encodeAgentSnapshot(snapshot)).toMatchObject({
      revision: MAX_U64,
      source_info: { observed_at_ms: MAX_U64 },
      applied_surface: { revision: MAX_U64 },
    });
    expect(encodeAgentChangePage(page).changes[0]).toMatchObject({
      occurred_at_ms: MAX_U64,
      payload: { applied: { revision: MAX_U64 } },
    });
  });

  it("round-trips callback generation and absolute deadline", () => {
    const invocation = decodeAgentToolInvocation({
      meta: {
        route_id: "route-1",
        binding_generation: MAX_U64,
        source: "source-1",
        turn_id: "turn-1",
        item_id: null,
        interaction_id: null,
        effect_id: "effect-1",
        idempotency_key: "idem-1",
        deadline_at_ms: MAX_U64,
      },
      tool: "workspace.inspect",
      arguments: {},
    });

    expect(invocation.meta.deadline_at_ms).toBe(MAX_U64_BIGINT);
    expect(encodeAgentToolInvocation(invocation).meta).toMatchObject({
      binding_generation: MAX_U64,
      deadline_at_ms: MAX_U64,
    });
  });

  it.each([1, "01", "-1", "18446744073709551616"])(
    "rejects non-canonical service u64 %s",
    (value) => {
      expect(() => decodeAgentServiceU64(value)).toThrow("expected");
    },
  );
});

describe("Runtime Wire canonical frame coordinates", () => {
  it.each([
    {
      kind: "request",
      payload: { method: "runtime_read", params: { thread_id: "thread-1" } },
    },
    {
      kind: "response",
      payload: {
        request_frame_id: MAX_U64,
        response: {
          method: "runtime_read",
          result: {
            status: "error",
            value: { code: "unavailable", message: "offline", retryable: true },
          },
        },
      },
    },
    {
      kind: "notification",
      payload: {
        kind: "heartbeat",
        payload: { last_received_frame_id: MAX_U64 },
      },
    },
    {
      kind: "ack",
      payload: { through_frame_id: MAX_U64 },
    },
  ])("round-trips $kind frame ids without numeric coercion", (frame) => {
    const decoded = decodeRuntimeWireEnvelope({
      protocol_revision: 4,
      frame_id: MAX_U64,
      critical: true,
      frame,
    });
    const encoded = encodeRuntimeWireEnvelope(decoded);

    expect(decoded.frame_id).toBe(MAX_U64_BIGINT);
    expect(encoded.frame_id).toBe(MAX_U64);
    if (decoded.frame.kind === "response") {
      expect(decoded.frame.payload.request_frame_id).toBe(MAX_U64_BIGINT);
    }
    if (decoded.frame.kind === "notification" && decoded.frame.payload.kind === "heartbeat") {
      expect(decoded.frame.payload.payload.last_received_frame_id).toBe(MAX_U64_BIGINT);
    }
    if (decoded.frame.kind === "ack") {
      expect(decoded.frame.payload.through_frame_id).toBe(MAX_U64_BIGINT);
    }
  });
});
