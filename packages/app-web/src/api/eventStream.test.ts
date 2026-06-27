import { describe, expect, it } from "vitest";
import {
  parseProjectEventStreamEnvelope,
  readProjectEventStreamCursor,
} from "./eventStream";

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

  it("keeps Project stream isolated from Session NDJSON envelope shape", () => {
    const event = parseProjectEventStreamEnvelope({
      type: "connected",
      last_event_id: 42,
    });

    expect(event).toBeNull();
  });

  it("rejects malformed StateChanged payloads", () => {
    const event = parseProjectEventStreamEnvelope({
      type: "StateChanged",
      data: {
        id: 7,
        project_id: "project-1",
        entity_id: "story-1",
        created_at: "2026-06-21T00:00:00Z",
      },
    });

    expect(event).toBeNull();
  });
});
