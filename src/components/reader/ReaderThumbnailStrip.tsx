import { useEffect, useRef, useState } from "react";
import type { PDFDocumentProxy } from "../../pdf";

/**
 * iOS 底部缩略页胶片（Apple Books 风格，窗口式）。
 *
 * 内存预算（docs/IOSUI_验收标准.md 修订 3，必须全部满足）：
 * - 缩略图渲染宽度 ≤ 150 物理像素；
 * - 渲染完成立即 toDataURL("image/jpeg", 0.7) 并销毁 canvas，DOM 只挂 <img>；
 * - 缓存 LRU 上限 24 张，换书清空；
 * - 渲染排在主页面之后（空闲时机启动），翻页立即作废未完成任务。
 */

const THUMB_MAX_WIDTH = 150; // 物理像素上限
const WINDOW = 5; // 当前页 ±5
const CACHE_MAX = 24; // LRU 张数上限
const JPEG_QUALITY = 0.7;

interface Props {
  doc: PDFDocumentProxy | null;
  page: number;
  numPages: number;
  onJump: (page: number) => void;
}

export default function ReaderThumbnailStrip({ doc, page, numPages, onJump }: Props) {
  /** 窗口内可展示的 dataURL（page → url），从 LRU 缓存投影而来 */
  const [thumbs, setThumbs] = useState<ReadonlyMap<number, string>>(new Map());
  /** LRU 缓存：Map 的插入序即访问序，最旧的在头部 */
  const cacheRef = useRef<Map<number, string>>(new Map());
  /** 任务代际：翻页/换书自增，旧任务据此自弃 */
  const generationRef = useRef(0);
  const stripRef = useRef<HTMLDivElement>(null);

  // 换书：清空缓存与展示（修订 3）
  useEffect(() => {
    generationRef.current++;
    cacheRef.current.clear();
    setThumbs(new Map());
  }, [doc]);

  useEffect(() => {
    if (!doc || numPages === 0) return;
    const gen = ++generationRef.current;
    const lo = Math.max(1, page - WINDOW);
    const hi = Math.min(numPages, page + WINDOW);

    const cache = cacheRef.current;
    const syncVisible = () => {
      const next = new Map<number, string>();
      for (let p = lo; p <= hi; p++) {
        const url = cache.get(p);
        if (url) {
          // touch：重插到尾部，维持 LRU 访问序
          cache.delete(p);
          cache.set(p, url);
          next.set(p, url);
        }
      }
      setThumbs(next);
    };
    syncVisible(); // 命中缓存的先亮

    // 从当前页向两侧展开的补齐顺序（近处优先）
    const missing: number[] = [];
    for (let d = 0; d <= WINDOW; d++) {
      for (const p of d === 0 ? [page] : [page + d, page - d]) {
        if (p >= lo && p <= hi && !cache.has(p)) missing.push(p);
      }
    }
    if (missing.length === 0) return;

    let cancelled = false;
    // 空闲时机启动，保证主页面渲染优先（修订 3）
    const win = window as Window & {
      requestIdleCallback?: (cb: () => void, opts?: { timeout: number }) => number;
    };
    const start = () => {
      void (async () => {
        for (const p of missing) {
          if (cancelled || generationRef.current !== gen) return;
          try {
            const proxy = await doc.getPage(p);
            if (cancelled || generationRef.current !== gen) return;
            const base = proxy.getViewport({ scale: 1 });
            const vp = proxy.getViewport({ scale: THUMB_MAX_WIDTH / base.width });
            const canvas = document.createElement("canvas");
            canvas.width = Math.floor(vp.width);
            canvas.height = Math.floor(vp.height);
            await proxy.render({
              canvasContext: canvas.getContext("2d")!,
              viewport: vp,
            }).promise;
            const stale = cancelled || generationRef.current !== gen;
            // 无论如何先取出并销毁 canvas，不留活位图
            const url = stale ? null : canvas.toDataURL("image/jpeg", JPEG_QUALITY);
            canvas.width = 0;
            canvas.height = 0;
            if (stale || url === null) return;
            cache.delete(p);
            cache.set(p, url);
            while (cache.size > CACHE_MAX) {
              const oldest = cache.keys().next().value;
              if (oldest === undefined) break;
              cache.delete(oldest);
            }
            syncVisible();
          } catch {
            /* 单页渲染失败或被取消：跳过，不阻塞其余缩略图 */
          }
        }
      })();
    };
    if (win.requestIdleCallback) win.requestIdleCallback(start, { timeout: 600 });
    else window.setTimeout(start, 250);

    return () => {
      cancelled = true;
    };
  }, [doc, page, numPages]);

  // 当前页缩略图滚动跟随（跳页后窗口居中）
  useEffect(() => {
    const el = stripRef.current?.querySelector<HTMLElement>(`[data-page="${page}"]`);
    el?.scrollIntoView({ inline: "center", block: "nearest", behavior: "smooth" });
  }, [page, thumbs]);

  if (!doc || numPages === 0) return null;
  const lo = Math.max(1, page - WINDOW);
  const hi = Math.min(numPages, page + WINDOW);
  const pages: number[] = [];
  for (let p = lo; p <= hi; p++) pages.push(p);

  return (
    <div className="reader-ios-strip" ref={stripRef} role="tablist" aria-label="页面缩略图">
      {pages.map((p) => {
        const url = thumbs.get(p);
        return (
          <button
            key={p}
            data-page={p}
            className={"reader-ios-strip-item" + (p === page ? " current" : "")}
            onClick={() => onJump(p)}
            aria-label={`第 ${p} 页`}
            aria-current={p === page ? "page" : undefined}
          >
            {url ? (
              <img src={url} alt="" draggable={false} />
            ) : (
              <span className="reader-ios-strip-placeholder" aria-hidden="true" />
            )}
            <span className="reader-ios-strip-num">{p}</span>
          </button>
        );
      })}
    </div>
  );
}
