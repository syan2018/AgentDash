import fs from "node:fs/promises";
import path from "node:path";
import vm from "node:vm";
import { pathToFileURL } from "node:url";

import { log } from "./protocol.mjs";

export async function loadExtension(bundlePath) {
  const source = await fs.readFile(bundlePath, "utf8");
  const moduleUrl = pathToFileURL(path.resolve(bundlePath)).href;
  const context = vm.createContext({
    console: {
      log: (...args) => log("info", args.join(" ")),
      warn: (...args) => log("warn", args.join(" ")),
      error: (...args) => log("error", args.join(" ")),
    },
    setTimeout,
    clearTimeout,
    structuredClone,
    TextDecoder,
    TextEncoder,
  });
  const module = new vm.SourceTextModule(source, {
    context,
    identifier: `${moduleUrl}?t=${Date.now()}`,
    initializeImportMeta(meta) {
      meta.url = moduleUrl;
    },
    importModuleDynamically(specifier) {
      throw new Error(`extension bundle must be self-contained; dynamic import blocked: ${specifier}`);
    },
  });
  await module.link((specifier) => {
    throw new Error(`extension bundle must be self-contained; import blocked: ${specifier}`);
  });
  await module.evaluate();
  const exported = module.namespace.default ?? module.namespace.extension;
  if (!exported || typeof exported !== "object") {
    throw new Error("extension bundle must export a default extension object");
  }
  return exported;
}
