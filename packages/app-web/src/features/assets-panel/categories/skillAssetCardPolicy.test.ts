import { describe, expect, it } from "vitest";

import { resolveSkillAssetCardPolicy } from "./skillAssetCardPolicy";

describe("resolveSkillAssetCardPolicy", () => {
  it("将平台 builtin 资产呈现为只读查看入口", () => {
    expect(
      resolveSkillAssetCardPolicy({
        source: "builtin_seed",
        installed_source: undefined,
      }),
    ).toEqual({
      isBuiltin: true,
      detailKind: "view",
      primaryLabel: "查看",
      canPublish: false,
      canDelete: false,
    });
  });

  it("保留 user 资产的编辑、发布与删除入口", () => {
    expect(
      resolveSkillAssetCardPolicy({
        source: "user",
        installed_source: undefined,
      }),
    ).toEqual({
      isBuiltin: false,
      detailKind: "edit",
      primaryLabel: "编辑",
      canPublish: true,
      canDelete: true,
    });
  });
});
