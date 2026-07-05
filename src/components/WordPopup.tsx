import { useLayoutEffect, useRef, useState } from "react";
import { hasEnglishVoice, speakWord, type LookupResult } from "../dict";

export interface PopupAnchor {
  /** 选区矩形（client 坐标） */
  left: number;
  right: number;
  top: number;
  bottom: number;
}

interface Props {
  result: LookupResult;
  anchor: PopupAnchor;
  onClose: () => void;
}

const WIDTH = 320;
const GAP = 8;
const MARGIN = 8;
const MAX_LINES = 5;

export default function WordPopup({ result, anchor, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number; below: boolean } | null>(null);
  const [expanded, setExpanded] = useState(false);
  const canSpeak = hasEnglishVoice();

  const lines = (result.translation ?? "").split("\n").filter((l) => l.trim());
  const shownLines = expanded ? lines : lines.slice(0, MAX_LINES);
  const folded = lines.length > MAX_LINES && !expanded;

  // 定位：默认单词正上方居中；上方放不下翻到下方；水平方向收拢贴边
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const h = el.offsetHeight;
    const vw = window.innerWidth;
    const cx = (anchor.left + anchor.right) / 2;
    let left = cx - WIDTH / 2;
    left = Math.max(MARGIN, Math.min(left, vw - WIDTH - MARGIN));
    const below = anchor.top - h - GAP < MARGIN;
    const top = below ? anchor.bottom + GAP : anchor.top - h - GAP;
    setPos({ left, top, below });
  }, [anchor, result, expanded]);

  const arrowLeft = Math.max(
    16,
    Math.min((anchor.left + anchor.right) / 2 - (pos?.left ?? 0), WIDTH - 16)
  );

  return (
    <div
      ref={ref}
      className={"word-popup" + (pos?.below ? " below" : "")}
      style={{
        left: pos?.left ?? -9999,
        top: pos?.top ?? -9999,
        width: WIDTH,
        visibility: pos ? "visible" : "hidden",
      }}
      onMouseDown={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
    >
      <div className="word-popup-arrow" style={{ left: arrowLeft }} />
      <div className="word-popup-head">
        <span className="word-popup-word">{result.word}</span>
        {result.lemma && result.lemma.toLowerCase() !== result.word.toLowerCase() && (
          <span className="word-popup-lemma">→ {result.lemma}</span>
        )}
        {canSpeak && (
          <button
            className="word-popup-speak"
            title="朗读"
            onClick={() => speakWord(result.lemma ?? result.word)}
          >
            🔊
          </button>
        )}
        <button className="word-popup-close" title="关闭" onClick={onClose}>
          ✕
        </button>
      </div>
      {result.phonetic && <div className="word-popup-phonetic">/{result.phonetic}/</div>}
      {result.found ? (
        <div className="word-popup-body">
          {shownLines.map((l, i) => (
            <div key={i} className="word-popup-line">
              {l}
            </div>
          ))}
          {folded && (
            <button className="word-popup-more" onClick={() => setExpanded(true)}>
              展开全部 {lines.length} 条 ▾
            </button>
          )}
        </div>
      ) : (
        <div className="word-popup-body word-popup-notfound">未找到释义</div>
      )}
    </div>
  );
}
