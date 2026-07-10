export const isMac =
  typeof navigator !== "undefined" && /Mac/i.test(navigator.userAgent);

export function isModKey(e: { ctrlKey: boolean; metaKey: boolean }): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}

export const modKeyLabel = isMac ? "⌘" : "Ctrl";
