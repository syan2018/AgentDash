export function send(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

export function log(level, message) {
  send({ kind: "log", level, message: String(message) });
}

export function toJsonValue(value) {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  if (Array.isArray(value)) return value.map(toJsonValue);
  if (typeof value === "object") {
    const result = {};
    for (const [key, item] of Object.entries(value)) {
      if (typeof item !== "function" && typeof item !== "symbol" && typeof item !== "undefined") {
        result[key] = toJsonValue(item);
      }
    }
    return result;
  }
  return null;
}
