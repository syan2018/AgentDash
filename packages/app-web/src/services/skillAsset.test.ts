import { describe, expect, it } from "vitest";

import {
  buildSkillMarkdown,
  buildSkillYamlFrontmatter,
  draftFromSkillAsset,
  dtoFilesFromDraft,
  parseSkillMarkdown,
  SKILL_ASSET_UPLOAD_MAX_TOTAL_BYTES,
  updateSkillMarkdownFrontmatter,
  validateSkillAssetDraft,
  validateSkillAssetUploadFiles,
  type SkillAssetDraft,
} from "./skillAsset";
import type { SkillAssetDto } from "../types";

describe("skillAsset", () => {
  it("extracts editable draft from generated api dto", () => {
    const dto: SkillAssetDto = {
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
          content_kind: "text",
          size_bytes: 96,
          kind: "skill",
        },
        {
          path: "references/api.md",
          content: "API",
          content_kind: "text",
          size_bytes: 3,
          kind: "reference",
        },
        {
          path: "assets/logo.png",
          content_kind: "binary",
          mime_type: "image/png",
          size_bytes: 4,
          kind: "asset",
        },
      ],
      created_at: "2026-05-12T00:00:00Z",
      updated_at: "2026-05-12T00:00:00Z",
    };

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
      binary_files: [
        {
          path: "assets/logo.png",
          content_kind: "binary",
          mime_type: "image/png",
          size_bytes: 4,
          kind: "asset",
        },
      ],
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
      binary_files: [],
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
      binary_files: [],
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

  it("parses folded YAML description blocks", () => {
    const parsed = parseSkillMarkdown(
      [
        "---",
        "name: abc-slang",
        "description: >-",
        "  将非技术人员的口语自动翻译为",
        "  项目代码术语。",
        "disable-model-invocation: true",
        "---",
        "# Body",
        "",
      ].join("\n"),
    );

    expect(parsed).toMatchObject({
      name: "abc-slang",
      description: "将非技术人员的口语自动翻译为 项目代码术语。",
      disable_model_invocation: true,
    });
  });

  it("replaces multiline YAML description blocks without leaving stale lines", () => {
    const content = [
      "---",
      "name: abc-slang",
      "description: >-",
      "  旧描述第一行",
      "  旧描述第二行",
      "category: docs",
      "---",
      "# Body",
      "",
    ].join("\n");

    const updated = updateSkillMarkdownFrontmatter(content, {
      description: "新描述第一行\n新描述第二行",
    });

    expect(updated).toContain("description: |-\n  新描述第一行\n  新描述第二行");
    expect(updated).toContain("category: docs");
    expect(updated).not.toContain("旧描述");
    expect(parseSkillMarkdown(updated)).toMatchObject({
      description: "新描述第一行\n新描述第二行",
    });
  });

  it("rejects local skill uploads before the browser sends oversized multipart bodies", () => {
    const oversized = [
      { size: SKILL_ASSET_UPLOAD_MAX_TOTAL_BYTES + 1 },
    ] as File[];

    expect(() => validateSkillAssetUploadFiles(oversized)).toThrow("Skill 上传总大小不能超过");
    expect(() => validateSkillAssetUploadFiles([{ size: 1024 }] as File[])).not.toThrow();
  });
});
