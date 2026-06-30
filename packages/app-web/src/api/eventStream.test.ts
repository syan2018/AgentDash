import { describe, expect, it } from "vitest";
import {
  readProjectEventStreamCursor,
} from "./eventStream";
import {
  parseProjectEventStreamEnvelope,
  parseProjectEventStreamEnvelopeResult,
} from "./projectEventStreamEnvelopeValidator";

describe("project event stream route-local envelope", () => {
  it("parses Connected and advances the stream cursor", () => {
    const event = parseProjectEventStreamEnvelope({
      type: "Connected",
      data: { last_event_id: 42 },
    });

    expect(event).toEqual({
      type: "Connected",
      data: { last_event_id: 42 },
    });
    expect(event ? readProjectEventStreamCursor(event) : null).toBe(42);
  });

  it("parses StateChanged with JSON object payload", () => {
    const event = parseProjectEventStreamEnvelope({
      type: "StateChanged",
      data: {
        id: 7,
        project_id: "project-1",
        entity_id: "story-1",
        kind: "story_updated",
        payload: {
          story_id: "story-1",
          nested: { ok: true },
        },
        backend_id: "backend-1",
        created_at: "2026-06-21T00:00:00Z",
      },
    });

    expect(event).toEqual({
      type: "StateChanged",
      data: {
        id: 7,
        project_id: "project-1",
        entity_id: "story-1",
        kind: "story_updated",
        payload: {
          story_id: "story-1",
          nested: { ok: true },
        },
        backend_id: "backend-1",
        created_at: "2026-06-21T00:00:00Z",
      },
    });
    expect(event ? readProjectEventStreamCursor(event) : null).toBe(7);
  });

  it("does not duplicate the generated ProjectStateChangeKind union as a runtime allowlist", () => {
    const result = parseProjectEventStreamEnvelopeResult({
      type: "StateChanged",
      data: {
        id: 9,
        project_id: "project-1",
        entity_id: "story-1",
        kind: "future_backend_kind",
        payload: {},
        backend_id: null,
        created_at: "2026-06-21T00:00:00Z",
      },
    });

    if (!result.ok) {
      throw result.error;
    }
    if (result.kind !== "StateChanged") {
      throw new Error(`Expected StateChanged envelope, got ${result.kind}`);
    }

    expect(result.envelope.data.kind).toBe("future_backend_kind");
  });

  it("keeps Project stream isolated from Session NDJSON envelope shape", () => {
    const result = parseProjectEventStreamEnvelopeResult({
      type: "connected",
      last_event_id: 42,
      ephemeral_epoch: 1,
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.message).toContain("未知 Project event stream 类型");
    }
  });

  it("rejects malformed StateChanged payloads", () => {
    const result = parseProjectEventStreamEnvelopeResult({
      type: "StateChanged",
      data: {
        id: 7,
        project_id: "project-1",
        entity_id: "story-1",
        created_at: "2026-06-21T00:00:00Z",
      },
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.message).toContain("StateChanged");
    }
  });

  it("parses Heartbeat without advancing the stream cursor", () => {
    const result = parseProjectEventStreamEnvelopeResult({
      type: "Heartbeat",
      data: { timestamp: 100 },
    });

    if (!result.ok) {
      throw result.error;
    }

    expect(result.kind).toBe("Heartbeat");
    expect(result.envelope).toEqual({
      type: "Heartbeat",
      data: { timestamp: 100 },
    });
    expect(readProjectEventStreamCursor(result.envelope)).toBeNull();
  });

  it("rejects unknown Project stream envelope types", () => {
    const result = parseProjectEventStreamEnvelopeResult({
      type: "Unknown",
      data: { timestamp: 100 },
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.message).toContain("未知 Project event stream 类型");
    }
  });
});
