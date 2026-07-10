import { describe, expect, it } from "vitest";

import {
  clampComponentSize,
  ComponentEventRateGate,
  parseComponentMessage,
  validateComponentPayload,
  type ExtensionUiComponentDescriptor,
} from "./componentProtocol";

const descriptor: ExtensionUiComponentDescriptor = {
  component_key: "demo.card",
  contract_version: 1,
  renderer: { kind: "iframe", entry: "dist/components/demo.card/index.html" },
  props_schema: true,
  events_schema: { select: { type: "object", required: ["id"] } },
  state_projection_schema: true,
  slots: [],
  sizing: { min_width: 160, min_height: 120, max_width: 640, max_height: 480 },
  sandbox_profile: "isolated_v1",
};

describe("component protocol", () => {
  it("validates the declared event payload", () => {
    expect(validateComponentPayload(descriptor.events_schema.select, { id: "a" })).toBe(true);
    expect(validateComponentPayload(descriptor.events_schema.select, {})).toBe(false);
    expect(parseComponentMessage({
      channel: "agentdash.component.v1",
      kind: "event",
      request_id: "request-1",
      event_type: "select",
      payload: { id: "a" },
    })).not.toBeNull();
  });

  it("clamps renderer size to descriptor bounds", () => {
    expect(clampComponentSize(descriptor, 10, 900)).toEqual({ width: 160, height: 480 });
  });

  it("rejects event bursts beyond the scoped rate limit", () => {
    const gate = new ComponentEventRateGate();
    for (let index = 0; index < 30; index += 1) expect(gate.admit(100)).toBe(true);
    expect(gate.admit(100)).toBe(false);
    expect(gate.admit(1_101)).toBe(true);
  });

  it("rejects oversized event payloads", () => {
    expect(parseComponentMessage({
      channel: "agentdash.component.v1",
      kind: "event",
      request_id: "request-1",
      event_type: "select",
      payload: "x".repeat(70_000),
    })).toBeNull();
  });
});
