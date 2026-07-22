import { describe, expect, it } from "vitest";

import type { CanonicalConversationRecord } from "../../../generated/backbone-protocol";
import { presentationCoordinates } from "./useSessionStream";

function record(presentationId: string): CanonicalConversationRecord {
  return {
    presentation_id: presentationId,
    presentation: {
      durability: "durable",
    },
  } as CanonicalConversationRecord;
}

describe("canonical conversation hydration boundary", () => {
  it("keeps newly delivered durable records live after the initial baseline", () => {
    const coordinates = presentationCoordinates(
      [record("history-1"), record("live-2")],
      new Set(["history-1"]),
    );

    expect(coordinates.get("history-1")?.baseline).toBe(true);
    expect(coordinates.get("live-2")?.baseline).toBe(false);
  });

  it("promotes records into the baseline only when a new snapshot is admitted", () => {
    const records = [record("history-1"), record("history-2")];
    const coordinates = presentationCoordinates(
      records,
      new Set(records.map((item) => item.presentation_id)),
    );

    expect([...coordinates.values()].every((coordinate) => coordinate.baseline)).toBe(true);
  });
});
