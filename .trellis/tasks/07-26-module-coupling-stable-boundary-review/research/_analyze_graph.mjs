import fs from "node:fs";
import path from "node:path";

const crates = new Map();
for (const entry of fs.readdirSync("crates", { withFileTypes: true })) {
  if (!entry.isDirectory()) continue;
  const manifestPath = path.join("crates", entry.name, "Cargo.toml");
  if (!fs.existsSync(manifestPath)) continue;
  const text = fs.readFileSync(manifestPath, "utf8");
  const packageName =
    text.match(/^\[package\][\s\S]*?^name\s*=\s*"([^"]+)"/m)?.[1] ?? entry.name;
  const dependencies = new Set();
  let section = "";
  for (const line of text.split(/\r?\n/)) {
    const heading = line.match(/^\[([^\]]+)\]\s*$/);
    if (heading) {
      section = heading[1];
      continue;
    }
    if (
      ["dependencies", "dev-dependencies", "build-dependencies"].includes(
        section,
      )
    ) {
      const dependency = line.match(/^(agentdash-[A-Za-z0-9_-]+)\s*=/)?.[1];
      if (dependency) dependencies.add(dependency);
    }
  }
  crates.set(packageName, { manifestPath, dependencies });
}

console.log("CRATE\tINTERNAL_DEPS\tFANOUT");
for (const [name, value] of [...crates].sort(
  (left, right) =>
    right[1].dependencies.size - left[1].dependencies.size ||
    left[0].localeCompare(right[0]),
)) {
  console.log(
    `${name}\t${[...value.dependencies].sort().join(",")}\t${value.dependencies.size}`,
  );
}

const reverse = new Map([...crates.keys()].map((name) => [name, new Set()]));
for (const [name, value] of crates) {
  for (const dependency of value.dependencies) {
    if (!reverse.has(dependency)) reverse.set(dependency, new Set());
    reverse.get(dependency).add(name);
  }
}
console.log("\nCRATE\tDEPENDENTS\tFANIN");
for (const [name, dependents] of [...reverse].sort(
  (left, right) =>
    right[1].size - left[1].size || left[0].localeCompare(right[0]),
)) {
  console.log(`${name}\t${[...dependents].sort().join(",")}\t${dependents.size}`);
}

let index = 0;
const stack = [];
const onStack = new Set();
const indices = new Map();
const lowLinks = new Map();
const components = [];
function visit(vertex) {
  indices.set(vertex, index);
  lowLinks.set(vertex, index);
  index += 1;
  stack.push(vertex);
  onStack.add(vertex);
  for (const neighbor of crates.get(vertex).dependencies) {
    if (!crates.has(neighbor)) continue;
    if (!indices.has(neighbor)) {
      visit(neighbor);
      lowLinks.set(vertex, Math.min(lowLinks.get(vertex), lowLinks.get(neighbor)));
    } else if (onStack.has(neighbor)) {
      lowLinks.set(vertex, Math.min(lowLinks.get(vertex), indices.get(neighbor)));
    }
  }
  if (lowLinks.get(vertex) === indices.get(vertex)) {
    const component = [];
    while (true) {
      const neighbor = stack.pop();
      onStack.delete(neighbor);
      component.push(neighbor);
      if (neighbor === vertex) break;
    }
    components.push(component);
  }
}
for (const vertex of crates.keys()) {
  if (!indices.has(vertex)) visit(vertex);
}
console.log("\nSCC>1");
for (const component of components) {
  if (component.length > 1) console.log(component.sort().join(","));
}
