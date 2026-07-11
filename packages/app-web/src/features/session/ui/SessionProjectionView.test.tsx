import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { RuntimeContextView } from "../../../generated/agent-runtime-contracts";
import { shouldApplyRuntimeContextResponse } from "../model/runtimeContextRequest";
import { SessionProjectionViewPanel } from "./SessionProjectionView";

function sampleContext(): RuntimeContextView {
  return {
    thread_id: "thread-1",
    fidelity: "driver_exact",
    head: {
      checkpoint_id: "checkpoint-1",
      revision: 4n,
      digest: "digest-1",
      fidelity: "driver_exact",
      provenance: { settings_revision: 2n, tool_set_revision: 3n },
    },
    checkpoint: {
      checkpoint_id: "checkpoint-1",
      thread_id: "thread-1",
      revision: 4n,
      materialized: {
        digest: "digest-1",
        fidelity: "driver_exact",
        recipe: {
          revision: 5n,
          provenance: { settings_revision: 2n, tool_set_revision: 3n },
          source_item_ids: ["item-1"],
        },
        blocks: [{ kind: "instruction", text: "保持回答简洁" }],
      },
    },
    blocks: [
      { kind: "instruction", text: "保持回答简洁" },
      { kind: "compaction_summary", summary: "此前讨论摘要" },
    ],
  };
}

describe("SessionProjectionViewPanel", () => {
  it("renders the canonical Runtime context head, checkpoint and blocks", () => {
    const html = renderToStaticMarkup(<SessionProjectionViewPanel context={sampleContext()} />);

    expect(html).toContain("driver_exact");
    expect(html).toContain("checkpoint-1");
    expect(html).toContain("保持回答简洁");
    expect(html).toContain("此前讨论摘要");
  });

  it("rejects a stale A response after the view switches to target B", () => {
    const requestA = { target_key: "run-a:agent-a", generation: 1 };
    const requestB = { target_key: "run-b:agent-b", generation: 2 };

    expect(shouldApplyRuntimeContextResponse(false, requestB, requestA)).toBe(false);
    expect(shouldApplyRuntimeContextResponse(true, requestB, requestA)).toBe(false);
    expect(shouldApplyRuntimeContextResponse(true, requestB, requestB)).toBe(true);
  });
});
