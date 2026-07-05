import { invoke } from "@tauri-apps/api/core";

export interface LookupResult {
  found: boolean;
  word: string;
  lemma: string | null;
  phonetic: string | null;
  translation: string | null;
}

/** 词典源统一接口。v0.2.0 只有本地实现；未来接在线源时弹窗 UI 零改动。 */
export interface DictSource {
  lookup(word: string): Promise<LookupResult>;
}

export const localDictSource: DictSource = {
  lookup: (word) => invoke<LookupResult>("lookup_word", { word }),
};

/**
 * 清洗双击选中的文本：剥首尾标点后校验是否为单个英文单词。
 * 返回 null 表示不可查询（中文/数字/多词/超长）。
 */
export function cleanWord(raw: string): string | null {
  const stripped = raw.trim().replace(/^[^A-Za-z]+|[^A-Za-z]+$/g, "");
  if (!stripped || stripped.length > 50) return null;
  if (!/^[A-Za-z][A-Za-z'-]*$/.test(stripped)) return null;
  return stripped;
}

// ---------- 发音（WebView2 内置合成语音，离线） ----------

let cachedVoice: SpeechSynthesisVoice | null | undefined;

function findEnglishVoice(): SpeechSynthesisVoice | null {
  const voices = window.speechSynthesis?.getVoices() ?? [];
  return (
    voices.find((v) => v.lang === "en-US") ??
    voices.find((v) => v.lang.startsWith("en")) ??
    null
  );
}

/** 是否有可用英文语音（无则弹窗内隐藏发音按钮） */
export function hasEnglishVoice(): boolean {
  if (!("speechSynthesis" in window)) return false;
  if (cachedVoice === undefined) {
    cachedVoice = findEnglishVoice();
    // voice 列表异步加载，首次为空时监听更新
    if (!cachedVoice) {
      window.speechSynthesis.addEventListener(
        "voiceschanged",
        () => {
          cachedVoice = findEnglishVoice();
        },
        { once: true }
      );
    }
  }
  return cachedVoice !== null;
}

export function speakWord(word: string) {
  if (!hasEnglishVoice() || !cachedVoice) return;
  const u = new SpeechSynthesisUtterance(word);
  u.voice = cachedVoice;
  u.lang = cachedVoice.lang;
  u.rate = 0.9;
  window.speechSynthesis.cancel(); // 支持连续点击重复播放
  window.speechSynthesis.speak(u);
}
