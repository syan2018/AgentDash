// @ts-check

import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

/**
 * @param {string} targetDir
 * @param {{ packageName?: string, extensionId?: string }} [options]
 */
export async function initProject(targetDir, options = {}) {
  const root = path.resolve(targetDir);
  const packageName = options.packageName ?? "@agentdash/local-hello";
  const extensionId = options.extensionId ?? packageName.split("/").pop() ?? "local-hello";
  await mkdir(path.join(root, "src", "panel"), { recursive: true });
  await writeFile(path.join(root, "package.json"), packageJson(packageName));
  await writeFile(path.join(root, "tsconfig.json"), tsconfigJson());
  await writeFile(path.join(root, "agentdash.extension.json"), manifestJson(packageName, extensionId));
  await writeFile(path.join(root, "src", "extension.ts"), extensionSource(packageName, extensionId));
  await writeFile(path.join(root, "src", "panel", "index.html"), panelHtml(extensionId));
  await writeFile(path.join(root, "src", "panel", "main.tsx"), panelMainSource(extensionId));
  await writeFile(path.join(root, "src", "panel", "App.tsx"), panelAppSource(extensionId));
}

/**
 * @param {string} packageName
 * @returns {string}
 */
function packageJson(packageName) {
  return `${JSON.stringify(
    {
      name: packageName,
      version: "0.1.0",
      type: "module",
      private: true,
      devDependencies: {
        "@agentdash/extension-dev": "workspace:*",
        "@agentdash/extension-sdk": "workspace:*",
        "@agentdash/extension-ui": "workspace:*",
        "@types/react": "^19.2.7",
        "@types/react-dom": "^19.2.3",
        react: "^19.2.0",
        "react-dom": "^19.2.0",
        typescript: "~5.9.3",
      },
      scripts: {
        dev: "agentdash-ext dev",
        validate: "agentdash-ext validate",
        pack: "agentdash-ext pack",
        "agentdash:install": "agentdash-ext install",
      },
    },
    null,
    2,
  )}\n`;
}

/**
 * @returns {string}
 */
function tsconfigJson() {
  return `${JSON.stringify(
    {
      compilerOptions: {
        target: "ES2022",
        lib: ["ES2022", "DOM"],
        module: "ESNext",
        moduleResolution: "bundler",
        jsx: "react-jsx",
        strict: true,
        skipLibCheck: true,
      },
      include: ["src"],
    },
    null,
    2,
  )}\n`;
}

/**
 * @param {string} packageName
 * @param {string} extensionId
 * @returns {string}
 */
function manifestJson(packageName, extensionId) {
  return `${JSON.stringify(
    {
      manifest_version: "2",
      extension_id: extensionId,
      package: { name: packageName, version: "0.1.0" },
      asset_version: "0.1.0",
      commands: [
        {
          name: `${extensionId}.hello`,
          description: "Send a hello message",
          handler: { kind: "inject_message", content: "Hello from AgentDash extension" },
        },
      ],
      runtime_actions: [
        {
          action_key: `${extensionId}.profile`,
          kind: "session_runtime",
          description: "Read local profile",
          input_schema: {},
          output_schema: {},
          permissions: ["local.profile.read"],
        },
      ],
      workspace_tabs: [
        {
          type_id: `${extensionId}.panel`,
          label: "Hello",
          uri_scheme: extensionId,
          renderer: { kind: "webview", entry: "dist/panel/index.html" },
        },
      ],
      permissions: [{ kind: "local_profile", access: "read" }],
      bundles: [
        {
          kind: "extension_host",
          entry: "dist/extension.js",
          digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        },
      ],
    },
    null,
    2,
  )}\n`;
}

/**
 * @param {string} packageName
 * @param {string} extensionId
 * @returns {string}
 */
function extensionSource(packageName, extensionId) {
  return `import { defineExtension } from "@agentdash/extension-sdk";

export default defineExtension({
  manifest: {
    manifest_version: "2",
    extension_id: ${JSON.stringify(extensionId)},
    package: { name: ${JSON.stringify(packageName)}, version: "0.1.0" },
    asset_version: "0.1.0",
  },
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: ${JSON.stringify(`${extensionId}.profile`)},
      kind: "session_runtime",
      description: "Read local profile",
      input_schema: {},
      output_schema: {},
      async invoke() {
        return await ctx.api.local.getProfile();
      },
    });
  },
});
`;
}

/**
 * @param {string} extensionId
 * @returns {string}
 */
function panelHtml(extensionId) {
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <title>${extensionId}</title>
  </head>
  <body>
    <main id="root">AgentDash extension panel</main>
    <script type="module" src="./main.js"></script>
  </body>
</html>
`;
}

/**
 * @param {string} extensionId
 * @returns {string}
 */
function panelMainSource(extensionId) {
  return `import React from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";

const root = document.getElementById("root");
if (!root) {
  throw new Error("AgentDash extension panel root element is missing");
}

createRoot(root).render(
  <React.StrictMode>
    <App actionKey=${JSON.stringify(`${extensionId}.profile`)} />
  </React.StrictMode>,
);
`;
}

/**
 * @param {string} extensionId
 * @returns {string}
 */
function panelAppSource(extensionId) {
  return `import { useEffect, useMemo, useState } from "react";
import { createExtensionBridge, type JsonObject } from "@agentdash/extension-ui";

interface AppProps {
  actionKey: string;
}

type ProfileState =
  | { status: "loading" }
  | { status: "ready"; profile: JsonObject }
  | { status: "error"; message: string };

export function App({ actionKey }: AppProps) {
  const bridge = useMemo(() => createExtensionBridge(), []);
  const [state, setState] = useState<ProfileState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    bridge.invokeAction<Record<string, never>, JsonObject>(actionKey, {})
      .then((profile) => {
        if (!cancelled) setState({ status: "ready", profile });
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setState({ status: "error", message: error instanceof Error ? error.message : "Profile request failed" });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [actionKey, bridge]);

  if (state.status === "loading") {
    return <main>Loading ${extensionId} profile...</main>;
  }

  if (state.status === "error") {
    return (
      <main>
        <h1>${extensionId}</h1>
        <p role="alert">{state.message}</p>
      </main>
    );
  }

  return (
    <main>
      <h1>${extensionId}</h1>
      <dl>
        <dt>Username</dt>
        <dd>{textField(state.profile.username)}</dd>
        <dt>Platform</dt>
        <dd>{textField(state.profile.platform)}</dd>
        <dt>Backend</dt>
        <dd>{textField(state.profile.backend_id)}</dd>
        <dt>Session</dt>
        <dd>{textField(state.profile.session_id)}</dd>
      </dl>
    </main>
  );
}

function textField(value: unknown): string {
  return typeof value === "string" && value.trim() !== "" ? value : "unknown";
}
`;
}
