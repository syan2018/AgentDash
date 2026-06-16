import { describe, expect, it } from "vitest";
import { activeFilterCount, selectFilteredStories } from "./select-stories";
import type { Story } from "../../types";

const baseContext = {
  source_refs: [],
  context_containers: [],
  disabled_container_ids: [],
  session_composition: null,
};

function makeStory(overrides: Partial<Story>): Story {
  return {
    id: "id-" + (overrides.title ?? "x"),
    project_id: "p1",
    default_workspace_id: null,
    title: "Sample",
    description: "",
    status: "created",
    priority: "p2",
    story_type: "feature",
    tags: [],
    context: baseContext,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

const params = {
  search: "",
  scope: "all" as const,
  statusFilter: "all" as const,
  priorityFilter: "all" as const,
  typeFilter: "all" as const,
  sort: "priority" as const,
};

describe("selectFilteredStories", () => {
  it("filters by scope:active excluding completed/cancelled", () => {
    const stories = [
      makeStory({ title: "a", status: "executing" }),
      makeStory({ title: "b", status: "completed" }),
      makeStory({ title: "c", status: "cancelled" }),
    ];
    const result = selectFilteredStories(stories, { ...params, scope: "active" });
    expect(result.map((s) => s.title)).toEqual(["a"]);
  });

  it("filters by scope:done keeping only completed/cancelled", () => {
    const stories = [
      makeStory({ title: "a", status: "executing" }),
      makeStory({ title: "b", status: "completed" }),
      makeStory({ title: "c", status: "cancelled" }),
    ];
    const result = selectFilteredStories(stories, { ...params, scope: "done" });
    expect(result.map((s) => s.title).sort()).toEqual(["b", "c"]);
  });

  it("matches by keyword across title/description/tags", () => {
    const stories = [
      makeStory({ title: "alpha", description: "" }),
      makeStory({ title: "beta", description: "matches alpha" }),
      makeStory({ title: "gamma", description: "" }),
      makeStory({ title: "delta", description: "", tags: ["alpha"] }),
    ];
    const result = selectFilteredStories(stories, { ...params, search: "alpha" });
    expect(result.map((s) => s.title).sort()).toEqual(["alpha", "beta", "delta"]);
  });

  it("filters by priority/type/status individually", () => {
    const stories = [
      makeStory({ title: "a", priority: "p0", story_type: "feature", status: "executing" }),
      makeStory({ title: "b", priority: "p1", story_type: "bugfix", status: "executing" }),
      makeStory({ title: "c", priority: "p0", story_type: "bugfix", status: "created" }),
    ];
    expect(
      selectFilteredStories(stories, { ...params, priorityFilter: "p0" }).map((s) => s.title).sort(),
    ).toEqual(["a", "c"]);
    expect(
      selectFilteredStories(stories, { ...params, typeFilter: "bugfix" }).map((s) => s.title).sort(),
    ).toEqual(["b", "c"]);
    expect(
      selectFilteredStories(stories, { ...params, statusFilter: "created" }).map((s) => s.title),
    ).toEqual(["c"]);
  });

  it("sorts by priority weight then updated_at desc", () => {
    const stories = [
      makeStory({ title: "p1-old", priority: "p1", updated_at: "2026-01-01T00:00:00Z" }),
      makeStory({ title: "p0-new", priority: "p0", updated_at: "2026-02-01T00:00:00Z" }),
      makeStory({ title: "p1-new", priority: "p1", updated_at: "2026-03-01T00:00:00Z" }),
      makeStory({ title: "p3", priority: "p3", updated_at: "2026-05-01T00:00:00Z" }),
    ];
    const result = selectFilteredStories(stories, { ...params, sort: "priority" });
    expect(result.map((s) => s.title)).toEqual(["p0-new", "p1-new", "p1-old", "p3"]);
  });

  it("sorts by title alphabetically", () => {
    const stories = [
      makeStory({ title: "banana" }),
      makeStory({ title: "apple" }),
      makeStory({ title: "cherry" }),
    ];
    const result = selectFilteredStories(stories, { ...params, sort: "title" });
    expect(result.map((s) => s.title)).toEqual(["apple", "banana", "cherry"]);
  });

  it("sorts by updated_at desc", () => {
    const stories = [
      makeStory({ title: "old", updated_at: "2026-01-01T00:00:00Z" }),
      makeStory({ title: "new", updated_at: "2026-05-01T00:00:00Z" }),
      makeStory({ title: "mid", updated_at: "2026-03-01T00:00:00Z" }),
    ];
    const result = selectFilteredStories(stories, { ...params, sort: "updated" });
    expect(result.map((s) => s.title)).toEqual(["new", "mid", "old"]);
  });
});

describe("activeFilterCount", () => {
  it("counts each active filter dimension", () => {
    expect(activeFilterCount(params)).toBe(0);
    expect(activeFilterCount({ ...params, search: "foo" })).toBe(1);
    expect(
      activeFilterCount({
        ...params,
        search: "foo",
        statusFilter: "executing",
        priorityFilter: "p0",
        typeFilter: "bugfix",
      }),
    ).toBe(4);
  });

  it("ignores whitespace-only search", () => {
    expect(activeFilterCount({ ...params, search: "   " })).toBe(0);
  });
});
