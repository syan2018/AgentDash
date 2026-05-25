import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { SessionLineageViewResponse } from "../../../generated/session-contracts";
import { SessionLineageViewPanel } from "./SessionLineageView";

describe("SessionLineageViewPanel", () => {
  it("渲染 fork source、branch status 和 child 列表", () => {
    const markup = renderToStaticMarkup(
      <SessionLineageViewPanel lineage={sampleLineage()} />,
    );

    expect(markup).toContain("BRANCH");
    expect(markup).toContain("child branch");
    expect(markup).toContain("fork");
    expect(markup).toContain("open");
    expect(markup).toContain("fork #42");
    expect(markup).toContain("parent sess-parent");
    expect(markup).toContain("sess-child-2");
  });
});

function sampleLineage(): SessionLineageViewResponse {
  return {
    session_id: "sess-child-1",
    lineage: {
      child_session_id: "sess-child-1",
      parent_session_id: "sess-parent",
      relation_kind: "fork",
      fork_point_event_seq: 42,
      fork_point_ref_json: {},
      fork_point_compaction_id: "fork-initial-sess-child-1",
      status: "open",
      created_at_ms: 1,
      updated_at_ms: 1,
      metadata_json: {},
    },
    ancestors: [
      {
        child_session_id: "sess-child-1",
        parent_session_id: "sess-parent",
        relation_kind: "fork",
        fork_point_event_seq: 42,
        fork_point_ref_json: {},
        status: "open",
        created_at_ms: 1,
        updated_at_ms: 1,
        metadata_json: {},
      },
    ],
    children: [
      {
        child_session_id: "sess-child-2",
        parent_session_id: "sess-child-1",
        relation_kind: "rollback_branch",
        fork_point_event_seq: 50,
        fork_point_ref_json: {},
        status: "open",
        created_at_ms: 2,
        updated_at_ms: 2,
        metadata_json: {},
      },
    ],
  };
}
