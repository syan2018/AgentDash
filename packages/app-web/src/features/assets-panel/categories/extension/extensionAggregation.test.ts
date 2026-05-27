import { describe, expect, it } from "vitest";

import type {
  ExtensionInstallationProjectionResponse,
  ExtensionRuntimeProjectionResponse,
} from "../../../../generated/extension-runtime-contracts";

import {
  aggregateInstalledExtensions,
  classifyInstallationSource,
  resolveInstallationVersion,
} from "./extensionAggregation";

function installation(
  key: string,
  overrides: Partial<ExtensionInstallationProjectionResponse> = {},
): ExtensionInstallationProjectionResponse {
  return {
    installation_id: `installation-${key}`,
    extension_key: key,
    extension_id: key,
    display_name: `Display ${key}`,
    installed_source: null,
    package_artifact: null,
    ...overrides,
  };
}

function emptyProjection(
  installations: ExtensionInstallationProjectionResponse[],
): ExtensionRuntimeProjectionResponse {
  return {
    installations,
    commands: [],
    flags: [],
    message_renderers: [],
    runtime_actions: [],
    protocol_channels: [],
    extension_dependencies: [],
    workspace_tabs: [],
    permissions: [],
    bundles: [],
  };
}

describe("classifyInstallationSource", () => {
  it("returns marketplace for installed_source only", () => {
    expect(
      classifyInstallationSource(
        installation("a", {
          installed_source: {
            library_asset_id: "asset-1",
            source_ref: "plugin:a",
            source_version: "0.1.0",
            source_digest: "sha256:digest",
            installed_at: "2026-05-26T00:00:00Z",
          },
        }),
      ),
    ).toBe("marketplace");
  });

  it("returns local_archive for package_artifact only", () => {
    expect(
      classifyInstallationSource(
        installation("a", {
          package_artifact: {
            artifact_id: "artifact-1",
            package_name: "@agentdash/a",
            package_version: "0.2.0",
            asset_version: "v1",
            source_version: "0.2.0",
            storage_ref: "ref",
            archive_digest: "sha256:abc",
            manifest_digest: "sha256:def",
          },
        }),
      ),
    ).toBe("local_archive");
  });

  it("returns marketplace_with_archive when both present", () => {
    expect(
      classifyInstallationSource(
        installation("a", {
          installed_source: {
            library_asset_id: "asset-1",
            source_ref: "plugin:a",
            source_version: "0.1.0",
            source_digest: "sha256:digest",
            installed_at: "2026-05-26T00:00:00Z",
          },
          package_artifact: {
            artifact_id: "artifact-1",
            package_name: "@agentdash/a",
            package_version: "0.2.0",
            asset_version: "v1",
            source_version: "0.2.0",
            storage_ref: "ref",
            archive_digest: "sha256:abc",
            manifest_digest: "sha256:def",
          },
        }),
      ),
    ).toBe("marketplace_with_archive");
  });

  it("returns unknown when neither is set", () => {
    expect(classifyInstallationSource(installation("a"))).toBe("unknown");
  });
});

describe("resolveInstallationVersion", () => {
  it("prefers package_artifact.package_version", () => {
    expect(
      resolveInstallationVersion(
        installation("a", {
          package_artifact: {
            artifact_id: "artifact-1",
            package_name: "@agentdash/a",
            package_version: "9.9.9",
            asset_version: "v1",
            source_version: "0.0.1",
            storage_ref: "ref",
            archive_digest: "sha256:abc",
            manifest_digest: "sha256:def",
          },
          installed_source: {
            library_asset_id: "asset-1",
            source_ref: "plugin:a",
            source_version: "0.0.1",
            source_digest: "sha256:digest",
            installed_at: "2026-05-26T00:00:00Z",
          },
        }),
      ),
    ).toBe("9.9.9");
  });

  it("falls back to installed_source.source_version", () => {
    expect(
      resolveInstallationVersion(
        installation("a", {
          installed_source: {
            library_asset_id: "asset-1",
            source_ref: "plugin:a",
            source_version: "1.2.3",
            source_digest: "sha256:digest",
            installed_at: "2026-05-26T00:00:00Z",
          },
        }),
      ),
    ).toBe("1.2.3");
  });

  it("returns empty string when version is unknown", () => {
    expect(resolveInstallationVersion(installation("a"))).toBe("");
  });
});

describe("aggregateInstalledExtensions joins related entries by extension_key", () => {
  it("groups permissions / tabs / actions / bundles by key", () => {
    const projection: ExtensionRuntimeProjectionResponse = {
      installations: [
        installation("alpha", {
          package_artifact: {
            artifact_id: "artifact-alpha",
            package_name: "@agentdash/alpha",
            package_version: "0.1.0",
            asset_version: "v1",
            source_version: "0.1.0",
            storage_ref: "ref",
            archive_digest: "sha256:alpha-archive",
            manifest_digest: "sha256:alpha-manifest",
          },
        }),
        installation("beta", {
          installed_source: {
            library_asset_id: "asset-beta",
            source_ref: "plugin:beta",
            source_version: "0.2.0",
            source_digest: "sha256:beta-digest",
            installed_at: "2026-05-26T00:00:00Z",
          },
        }),
        installation("gamma"),
      ],
      commands: [
        {
          extension_key: "alpha",
          extension_id: "alpha",
          name: "alpha:run",
          description: "",
          handler: { kind: "inject_message", content: "x" },
        },
      ],
      flags: [
        {
          extension_key: "beta",
          extension_id: "beta",
          name: "beta.flag",
          flag_type: "bool",
          default: false,
          description: "",
        },
      ],
      message_renderers: [
        {
          extension_key: "alpha",
          extension_id: "alpha",
          custom_type: "alpha.card",
          renderer: { kind: "json_card" },
        },
      ],
      runtime_actions: [
        {
          extension_key: "alpha",
          extension_id: "alpha",
          action_key: "alpha.action",
          kind: "session_runtime",
          description: "",
          input_schema: {},
          output_schema: {},
          permissions: [],
        },
        {
          extension_key: "beta",
          extension_id: "beta",
          action_key: "beta.action",
          kind: "session_runtime",
          description: "",
          input_schema: {},
          output_schema: {},
          permissions: [],
        },
      ],
      protocol_channels: [
        {
          extension_key: "alpha",
          extension_id: "alpha",
          channel_key: "alpha.api",
          version: "1.0.0",
          description: "",
          methods: [
            {
              name: "readProfile",
              description: "",
              input_schema: {},
              output_schema: {},
              permissions: [],
            },
          ],
        },
      ],
      extension_dependencies: [
        {
          extension_key: "beta",
          extension_id: "beta",
          dependency: {
            alias: "alpha",
            extension_id: "alpha",
            version: "^1.0.0",
            channels: ["alpha.api"],
          },
        },
      ],
      workspace_tabs: [
        {
          extension_key: "alpha",
          extension_id: "alpha",
          type_id: "alpha.panel",
          label: "Alpha",
          uri_scheme: "alpha",
          renderer: { kind: "webview", entry: "dist/index.html" },
        },
      ],
      permissions: [
        {
          extension_key: "alpha",
          extension_id: "alpha",
          permission: { kind: "local_profile", access: "read" },
        },
        {
          extension_key: "beta",
          extension_id: "beta",
          permission: { kind: "workspace", access: "read_write" },
        },
      ],
      bundles: [
        {
          extension_key: "alpha",
          extension_id: "alpha",
          kind: "extension_host",
          entry: "dist/extension.js",
          digest: "sha256:bundle-alpha",
        },
      ],
    };

    const rows = aggregateInstalledExtensions(projection);
    expect(rows).toHaveLength(3);

    const alpha = rows.find((row) => row.installation.extension_key === "alpha");
    expect(alpha).toBeDefined();
    expect(alpha?.source).toBe("local_archive");
    expect(alpha?.version).toBe("0.1.0");
    expect(alpha?.commands).toHaveLength(1);
    expect(alpha?.workspaceTabs).toHaveLength(1);
    expect(alpha?.runtimeActions).toHaveLength(1);
    expect(alpha?.protocolChannels).toHaveLength(1);
    expect(alpha?.extensionDependencies).toHaveLength(0);
    expect(alpha?.runtimeActions[0].action_key).toBe("alpha.action");
    expect(alpha?.permissions).toEqual([{ kind: "local_profile", access: "read" }]);
    expect(alpha?.bundle?.digest).toBe("sha256:bundle-alpha");
    expect(alpha?.messageRenderers).toHaveLength(1);
    expect(alpha?.flags).toHaveLength(0);

    const beta = rows.find((row) => row.installation.extension_key === "beta");
    expect(beta?.source).toBe("marketplace");
    expect(beta?.version).toBe("0.2.0");
    expect(beta?.flags).toHaveLength(1);
    expect(beta?.runtimeActions).toHaveLength(1);
    expect(beta?.protocolChannels).toHaveLength(0);
    expect(beta?.extensionDependencies).toHaveLength(1);
    expect(beta?.runtimeActions[0].action_key).toBe("beta.action");
    expect(beta?.workspaceTabs).toHaveLength(0);
    expect(beta?.bundle).toBeNull();
    expect(beta?.permissions).toEqual([{ kind: "workspace", access: "read_write" }]);

    const gamma = rows.find((row) => row.installation.extension_key === "gamma");
    expect(gamma?.source).toBe("unknown");
    expect(gamma?.version).toBe("");
    expect(gamma?.permissions).toHaveLength(0);
    expect(gamma?.workspaceTabs).toHaveLength(0);
    expect(gamma?.protocolChannels).toHaveLength(0);
    expect(gamma?.extensionDependencies).toHaveLength(0);
    expect(gamma?.bundle).toBeNull();
  });

  it("returns empty when projection has no installations", () => {
    expect(aggregateInstalledExtensions(emptyProjection([]))).toEqual([]);
  });
});
