// @ts-check

export { createExtensionContext } from "./runtime-context.js";

/**
 * @template {{ manifest?: unknown, activate?: unknown }} TExtension
 * @param {TExtension} extension
 * @returns {TExtension}
 */
export function defineExtension(extension) {
  return extension;
}
