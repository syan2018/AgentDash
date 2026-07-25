import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "../../packages/app-web/node_modules/vitest/dist/config.js";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const require = createRequire(resolve(repoRoot, "packages/app-web/package.json"));
const projectsRoot = resolve(repoRoot, "..");

export default defineConfig({
  root: projectsRoot,
  cacheDir: resolve(repoRoot, "target/vitest-main-reference"),
  plugins: [
    {
      name: "agentdash-main-reference-dependencies",
      enforce: "pre",
      resolveId(source) {
        if (
          source.startsWith(".") ||
          source.startsWith("/") ||
          source.startsWith("\0") ||
          source === "vitest"
        ) {
          return null;
        }
        try {
          return require.resolve(source);
        } catch {
          return null;
        }
      },
    },
  ],
  test: {
    environment: "node",
  },
});
