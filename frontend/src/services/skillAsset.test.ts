import { describe, expect, it } from "vitest";

import {
  buildSkillMarkdown,
  buildSkillYamlFrontmatter,
  draftFromSkillAsset,
  dtoFilesFromDraft,
  mapSkillAsset,
  parseSkillMarkdown,
  updateSkillMarkdownFrontmatter,
  validateSkillAssetDraft,
  type SkillAssetDraft,
} from "./skillAsset";

describe("skillAsset", () => {
  it("maps api dto and extracts editable draft from SKILL.md", () => {
    const dto = mapSkillAsset({
      id: "asset-1",
      project_id: "project-1",
      key: "research",
      display_name: "Research",
      description: "调研资料整理",
      source: "builtin_seed",
      builtin_key: "research",
      disable_model_invocation: true,
      files: [
        {
          path: "SKILL.md",
          content:
            '---\nname: research\ndescription: "调研资料整理"\ndisable-model-invocation: true\n---\n# Body\n',
          kind: "skill",
        },
        {
          path: "references/api.md",
          content: "API",
          kind: "reference",
        },
      ],
      created_at: "2026-05-12T00:00:00Z",
      updated_at: "2026-05-12T00:00:00Z",
    });

    expect(dto.source).toBe("builtin_seed");
    const draft = draftFromSkillAsset(dto);

    expect(draft).toMatchObject({
      id: "asset-1",
      key: "research",
      display_name: "Research",
      description: "调研资料整理",
      disable_model_invocation: true,
      body: "# Body\n",
      files: [{ relative_path: "references/api.md", content: "API" }],
    });
  });

  it("builds request files with SKILL.md frontmatter synchronized", () => {
    const draft: SkillAssetDraft = {
      key: "writer",
      display_name: "Writer",
      description: "写作辅助",
      body: "# Writer\n",
      disable_model_invocation: false,
      files: [{ relative_path: "references/style.md", content: "style" }],
    };

    expect(validateSkillAssetDraft(draft).ok).toBe(true);
    expect(buildSkillMarkdown(draft)).toContain("name: writer");
    expect(buildSkillYamlFrontmatter(draft)).toBe(
      '---\nname: writer\ndescription: "写作辅助"\n---',
    );
    expect(dtoFilesFromDraft(draft).map((file) => file.path)).toEqual([
      "SKILL.md",
      "references/style.md",
    ]);
  });

  it("rejects duplicate and unsafe paths before sending requests", () => {
    const draft: SkillAssetDraft = {
      key: "writer",
      display_name: "Writer",
      description: "写作辅助",
      body: "",
      disable_model_invocation: false,
      files: [
        { relative_path: "references/../bad.md", content: "" },
      ],
    };

    expect(validateSkillAssetDraft(draft).ok).toBe(false);
    expect(validateSkillAssetDraft({ ...draft, files: [] }, ["writer"]).ok).toBe(false);
  });

  it("updates SKILL.md frontmatter while preserving unknown metadata", () => {
    const content =
      '---\nname: writer\ndescription: "写作辅助"\ncategory: docs\ndisable-model-invocation: true\n---\n# Writer\n';

    const updated = updateSkillMarkdownFrontmatter(content, {
      description: "更新后的写作辅助",
      disable_model_invocation: false,
    });

    expect(updated).toContain('description: "更新后的写作辅助"');
    expect(updated).toContain("category: docs");
    expect(updated).not.toContain("disable-model-invocation");
    expect(parseSkillMarkdown(updated)).toMatchObject({
      name: "writer",
      description: "更新后的写作辅助",
      disable_model_invocation: false,
      body: "# Writer\n",
    });
  });
});
