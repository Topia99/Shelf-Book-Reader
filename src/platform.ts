export const isMac =
  typeof navigator !== "undefined" && /Mac/i.test(navigator.userAgent);

export const isTouchDevice =
  typeof window !== "undefined" &&
  typeof window.matchMedia === "function" &&
  window.matchMedia("(pointer: coarse)").matches;

export function isModKey(e: { ctrlKey: boolean; metaKey: boolean }): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}

export const modKeyLabel = isMac ? "⌘" : "Ctrl";
