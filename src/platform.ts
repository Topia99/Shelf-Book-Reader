const userAgent = typeof navigator !== "undefined" ? navigator.userAgent : "";
const maxTouchPoints = typeof navigator !== "undefined" ? navigator.maxTouchPoints : 0;

// iPadOS 在 Safari/WKWebView 里会伪装成 Macintosh。
// 这里单独把 “Macintosh + 多点触控” 识别成 iOS，避免阅读器误走桌面壳。
export const isIos =
  /iPhone|iPad|iPod/.test(userAgent) || (/Macintosh/.test(userAgent) && maxTouchPoints > 1);

export const isMac = /Mac/i.test(userAgent);

export const isTouchDevice =
  typeof window !== "undefined" &&
  typeof window.matchMedia === "function" &&
  window.matchMedia("(pointer: coarse)").matches;

export function isModKey(e: { ctrlKey: boolean; metaKey: boolean }): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}

export const modKeyLabel = isMac ? "⌘" : "Ctrl";
