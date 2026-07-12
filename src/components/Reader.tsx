import { useCallback, useEffect, useRef, useState } from "react";
import { saveCover, setTotalPages, updateProgress, type Book } from "../api";
import { isPasswordError, openPdf, pdfjs, renderCoverPng, type PDFDocumentProxy } from "../pdf";
import { cleanWord, localDictSource, type LookupResult } from "../dict";
import { isModKey } from "../platform";
import WordPopup, { type PopupAnchor } from "./WordPopup";

interface Props {
  book: Book;
  onBack: () => void;
}

type ZoomMode = "fit-width" | "fit-page" | "custom";

interface OutlineNode {
  title: string;
  page: number | null;
  children: OutlineNode[];
}

// WebKit 专有 gesture 事件（TS 无内置类型），继承 Event 以便从监听器参数直接窄化
interface WebKitGestureEvent extends Event {
  scale: number;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export default function Reader({ book, onBack }: Props) {
  const [doc, setDoc] = useState<PDFDocumentProxy | null>(null);
  const [numPages, setNumPages] = useState(0);
  const [page, setPage] = useState(1);
  const [zoom, setZoom] = useState<ZoomMode>("fit-page");
  const [customScale, setCustomScale] = useState(1);
  const [twoPage, setTwoPage] = useState(false);
  const [outline, setOutline] = useState<OutlineNode[]>([]);
  const [showOutline, setShowOutline] = useState(false);
  const [needPassword, setNeedPassword] = useState(false);
  const [passwordError, setPasswordError] = useState("");
  const [loadError, setLoadError] = useState("");
  const [pageInput, setPageInput] = useState("1");
  const [displayScale, setDisplayScale] = useState(1);
  const [containerSize, setContainerSize] = useState({ w: 0, h: 0 });
  const [popup, setPopup] = useState<{ result: LookupResult; anchor: PopupAnchor } | null>(null);
  const [toast, setToast] = useState("");

  const containerRef = useRef<HTMLDivElement>(null);
  const docRef = useRef<PDFDocumentProxy | null>(null);
  const canvas1Ref = useRef<HTMLCanvasElement>(null);
  const canvas2Ref = useRef<HTMLCanvasElement>(null);
  const text1Ref = useRef<HTMLDivElement>(null);
  const text2Ref = useRef<HTMLDivElement>(null);
  const wrap2Ref = useRef<HTMLDivElement>(null);
  const lastScaleRef = useRef(1);
  const gestureStartScaleRef = useRef(1);
  const pageRef = useRef(1);
  const lastFlipAtRef = useRef(0);
  const popupRef = useRef(popup);
  const isScannedRef = useRef(false);
  const scanToastShownRef = useRef(false);
  const popupJustClosedRef = useRef(false);
  const step = twoPage ? 2 : 1;

  popupRef.current = popup;

  // ---------- 文档加载（含密码重试） ----------
  const loadDoc = useCallback(
    async (password?: string) => {
      try {
        const d = await openPdf(book.file_path, password).promise;
        docRef.current?.destroy();
        docRef.current = d;
        setNeedPassword(false);
        setPasswordError("");
        setNumPages(d.numPages);
        const startPage = clamp(book.current_page || 1, 1, d.numPages);
        setPage(startPage);
        pageRef.current = startPage;
        setDoc(d);
        // 打开即记一次进度，让「最近阅读」排序生效
        updateProgress(book.id, startPage).catch(() => {});
        if (book.total_pages !== d.numPages) {
          setTotalPages(book.id, d.numPages).catch(() => {});
        }
        // 加密书首次成功打开时补生成封面
        if (!book.cover_path) {
          renderCoverPng(d)
            .then((png) => saveCover(book.hash, png))
            .catch(() => {});
        }
        loadOutline(d).then(setOutline);
        // 扫描版检测：前 1–2 页文字层近似为空则判定为扫描版（仅影响取词提示文案）
        detectScanned(d).then((scanned) => {
          isScannedRef.current = scanned;
        });
      } catch (e) {
        if (isPasswordError(e)) {
          setPasswordError(password ? "密码错误，请重试" : "");
          setNeedPassword(true);
        } else {
          setLoadError(`无法打开此 PDF：${e instanceof Error ? e.message : String(e)}`);
        }
      }
    },
    [book]
  );

  useEffect(() => {
    loadDoc();
    return () => {
      docRef.current?.destroy();
      docRef.current = null;
    };
  }, [loadDoc]);

  // ---------- 进度：翻页后 1 秒节流写库，退出时强制落盘 ----------
  useEffect(() => {
    pageRef.current = page;
    setPageInput(String(page));
    if (!doc) return;
    const t = setTimeout(() => updateProgress(book.id, page).catch(() => {}), 1000);
    return () => clearTimeout(t);
  }, [page, doc, book.id]);

  const flushProgress = useCallback(() => {
    updateProgress(book.id, pageRef.current).catch(() => {});
  }, [book.id]);

  useEffect(() => {
    window.addEventListener("beforeunload", flushProgress);
    return () => {
      window.removeEventListener("beforeunload", flushProgress);
      flushProgress();
    };
  }, [flushProgress]);

  const goBack = useCallback(() => {
    flushProgress();
    onBack();
  }, [flushProgress, onBack]);

  // ---------- 翻页 ----------
  const gotoPage = useCallback(
    (p: number) => {
      if (!numPages) return;
      setPage(clamp(p, 1, numPages));
      containerRef.current?.scrollTo({ top: 0 });
    },
    [numPages]
  );
  const next = useCallback(() => gotoPage(pageRef.current + step), [gotoPage, step]);
  const prev = useCallback(() => gotoPage(pageRef.current - step), [gotoPage, step]);

  // 键盘
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.target as HTMLElement)?.tagName === "INPUT") return;
      switch (e.key) {
        case "ArrowRight":
        case "PageDown":
        case " ":
          e.preventDefault();
          next();
          break;
        case "ArrowLeft":
        case "PageUp":
          e.preventDefault();
          prev();
          break;
        case "Home":
          gotoPage(1);
          break;
        case "End":
          gotoPage(numPages);
          break;
        case "Escape":
          if (popupRef.current) setPopup(null);
          else goBack();
          break;
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [next, prev, gotoPage, numPages, goBack]);

  // 滚轮：修饰键+滚轮缩放；普通滚轮先滚动页面内容，到边缘再翻页
  // macOS WKWebView 的触控板捏合会触发 WebKit 专有 gesture 事件，这里接管以复用现有缩放状态。
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    function onWheel(e: WheelEvent) {
      if (isModKey(e)) {
        e.preventDefault();
        const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
        setCustomScale(clamp(lastScaleRef.current * factor, 0.2, 6));
        setZoom("custom");
        return;
      }
      const c = el!;
      const atBottom = c.scrollTop + c.clientHeight >= c.scrollHeight - 1;
      const atTop = c.scrollTop <= 0;
      const now = Date.now();
      if ((e.deltaY > 0 && atBottom) || (e.deltaY < 0 && atTop)) {
        e.preventDefault();
        if (now - lastFlipAtRef.current < 300) return; // 防连滚误翻多页
        lastFlipAtRef.current = now;
        if (e.deltaY > 0) next();
        else prev();
      }
    }
    function onGestureStart(e: Event) {
      const gestureEvent = e as WebKitGestureEvent;
      gestureStartScaleRef.current = lastScaleRef.current;
      gestureEvent.preventDefault();
    }
    function onGestureChange(e: Event) {
      const gestureEvent = e as WebKitGestureEvent;
      setCustomScale(clamp(gestureStartScaleRef.current * gestureEvent.scale, 0.2, 6));
      setZoom("custom");
      gestureEvent.preventDefault();
    }
    function onGestureEnd(e: Event) {
      (e as WebKitGestureEvent).preventDefault();
    }
    el.addEventListener("wheel", onWheel, { passive: false });
    el.addEventListener("gesturestart", onGestureStart as EventListener, { passive: false });
    el.addEventListener("gesturechange", onGestureChange as EventListener, { passive: false });
    el.addEventListener("gestureend", onGestureEnd as EventListener, { passive: false });
    return () => {
      el.removeEventListener("wheel", onWheel);
      el.removeEventListener("gesturestart", onGestureStart as EventListener);
      el.removeEventListener("gesturechange", onGestureChange as EventListener);
      el.removeEventListener("gestureend", onGestureEnd as EventListener);
    };
  }, [next, prev]);

  // 翻页 / 缩放 / 视图切换时自动关闭取词弹窗
  useEffect(() => {
    setPopup(null);
  }, [page, zoom, customScale, twoPage]);

  // 页面内滚动时关闭弹窗（弹窗定位基于屏幕坐标，滚动后会错位）
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const onScroll = () => {
      if (popupRef.current) setPopup(null);
    };
    el.addEventListener("scroll", onScroll);
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  // 提示 toast 自动消失
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(""), 3000);
    return () => clearTimeout(t);
  }, [toast]);

  // ---------- 取词 ----------
  const onDoubleClick = useCallback(async () => {
    if (isScannedRef.current) {
      if (!scanToastShownRef.current) {
        scanToastShownRef.current = true;
        setToast("本书为扫描版，暂不支持取词");
      }
      return;
    }
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0) return;
    const word = cleanWord(sel.toString());
    if (!word) return;
    const rect = sel.getRangeAt(0).getBoundingClientRect();
    if (rect.width === 0 && rect.height === 0) return;
    try {
      const result = await localDictSource.lookup(word);
      setPopup({
        result,
        anchor: { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom },
      });
    } catch {
      setToast("查词失败，请重试");
    }
  }, []);

  // 点击弹窗以外区域关闭弹窗（弹窗内部 mousedown 已 stopPropagation）
  const onContainerMouseDown = useCallback(() => {
    if (popupRef.current) {
      setPopup(null);
      popupJustClosedRef.current = true;
    }
  }, []);

  // 点击左右翻页区（文字上的点击不翻页，留给选词）
  const onContainerClick = useCallback(
    (e: React.MouseEvent) => {
      if (popupJustClosedRef.current) {
        popupJustClosedRef.current = false;
        return;
      }
      if ((e.target as HTMLElement).closest(".textLayer")) return;
      const el = containerRef.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      const rel = (e.clientX - r.left) / r.width;
      if (rel < 0.22) prev();
      else if (rel > 0.78) next();
    },
    [prev, next]
  );

  // 容器尺寸变化时重新计算适配缩放
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      setContainerSize({ w: el.clientWidth, h: el.clientHeight });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // ---------- 渲染当前页（双页模式渲染两页） ----------
  useEffect(() => {
    if (!doc || !containerRef.current) return;
    let cancelled = false;
    const tasks: { cancel: () => void }[] = [];

    (async () => {
      const pageNums = twoPage && page + 1 <= numPages ? [page, page + 1] : [page];
      const proxies = await Promise.all(pageNums.map((p) => doc.getPage(p)));
      if (cancelled) return;

      const el = containerRef.current!;
      const padding = 24;
      const gap = 8;
      const cw = el.clientWidth - padding * 2 - (twoPage ? gap : 0);
      const ch = el.clientHeight - padding * 2;
      const base = proxies[0].getViewport({ scale: 1 });
      const perW = twoPage ? cw / 2 : cw;

      let scale: number;
      if (zoom === "fit-width") scale = perW / base.width;
      else if (zoom === "fit-page") scale = Math.min(perW / base.width, ch / base.height);
      else scale = customScale;
      scale = clamp(scale, 0.1, 8);
      lastScaleRef.current = scale;
      setDisplayScale(scale);

      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      // iOS WKWebView 的可用内存较紧，极端缩放叠加高 DPR 时容易生成超大位图并被系统杀掉。
      const MAX_CANVAS_PIXELS = 16_777_216;
      const canvases = [canvas1Ref.current, canvas2Ref.current];
      const textDivs = [text1Ref.current, text2Ref.current];
      if (wrap2Ref.current) {
        wrap2Ref.current.style.display = proxies.length > 1 ? "block" : "none";
      }

      for (let i = 0; i < proxies.length; i++) {
        const canvas = canvases[i];
        const textDiv = textDivs[i];
        if (!canvas || !textDiv || cancelled) return;
        const vpLayout = proxies[i].getViewport({ scale });
        const requestedRenderScale = scale * dpr;
        const requestedPixels = vpLayout.width * dpr * (vpLayout.height * dpr);
        const renderBudgetRatio =
          requestedPixels > MAX_CANVAS_PIXELS
            ? Math.sqrt(MAX_CANVAS_PIXELS / requestedPixels)
            : 1;
        const renderScale = requestedRenderScale * renderBudgetRatio;
        const vp = proxies[i].getViewport({ scale: renderScale });
        canvas.width = Math.floor(vp.width);
        canvas.height = Math.floor(vp.height);
        canvas.style.width = `${Math.floor(vpLayout.width)}px`;
        canvas.style.height = `${Math.floor(vpLayout.height)}px`;
        const task = proxies[i].render({
          canvasContext: canvas.getContext("2d")!,
          viewport: vp,
        });
        tasks.push(task);
        try {
          await task.promise;
        } catch {
          return; // 渲染被取消
        }
        if (cancelled) return;
        // 透明文字层：让双击可以选中单词（取词功能的基础）
        const vpText = proxies[i].getViewport({ scale });
        textDiv.replaceChildren();
        textDiv.style.width = `${Math.floor(vpLayout.width)}px`;
        textDiv.style.height = `${Math.floor(vpLayout.height)}px`;
        textDiv.style.setProperty("--scale-factor", String(scale));
        const textLayer = new pdfjs.TextLayer({
          textContentSource: proxies[i].streamTextContent(),
          container: textDiv,
          viewport: vpText,
        });
        tasks.push(textLayer);
        try {
          await textLayer.render();
        } catch {
          return; // 文字层渲染被取消（扫描版无文字层时正常完成但为空）
        }
      }
    })();

    return () => {
      cancelled = true;
      tasks.forEach((t) => t.cancel());
    };
  }, [doc, page, numPages, zoom, customScale, twoPage, containerSize]);

  function zoomBy(factor: number) {
    setCustomScale(clamp(lastScaleRef.current * factor, 0.2, 6));
    setZoom("custom");
  }

  function submitPageInput() {
    const n = parseInt(pageInput, 10);
    if (!isNaN(n)) gotoPage(n);
    else setPageInput(String(page));
  }

  // ---------- 密码 / 错误界面 ----------
  if (loadError) {
    return (
      <div className="reader-fallback">
        <p>{loadError}</p>
        <button className="btn" onClick={onBack}>
          返回书库
        </button>
      </div>
    );
  }
  if (needPassword) {
    return (
      <PasswordPrompt
        title={book.title}
        error={passwordError}
        onSubmit={(pwd) => loadDoc(pwd)}
        onCancel={onBack}
      />
    );
  }

  return (
    <div className="reader">
      <header className="reader-toolbar">
        <button className="btn" onClick={goBack} title="返回书库（Esc）">
          ← 书库
        </button>
        <button
          className={"btn" + (showOutline ? " active" : "")}
          onClick={() => setShowOutline((v) => !v)}
          disabled={outline.length === 0}
          title={outline.length === 0 ? "此 PDF 没有目录" : "目录"}
        >
          目录
        </button>
        <span className="reader-title">{book.title}</span>
        <div className="toolbar-group">
          <input
            className="page-input"
            value={pageInput}
            onChange={(e) => setPageInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submitPageInput()}
            onBlur={submitPageInput}
          />
          <span className="page-total">/ {numPages || "…"}</span>
        </div>
        <div className="toolbar-group">
          <button
            className={"btn" + (zoom === "fit-width" ? " active" : "")}
            onClick={() => setZoom("fit-width")}
          >
            适合宽度
          </button>
          <button
            className={"btn" + (zoom === "fit-page" ? " active" : "")}
            onClick={() => setZoom("fit-page")}
          >
            适合整页
          </button>
          <button className="btn" onClick={() => zoomBy(1 / 1.2)}>
            −
          </button>
          <span className="zoom-label">{Math.round(displayScale * 100)}%</span>
          <button className="btn" onClick={() => zoomBy(1.2)}>
            ＋
          </button>
        </div>
        <button
          className={"btn" + (twoPage ? " active" : "")}
          onClick={() => setTwoPage((v) => !v)}
        >
          {twoPage ? "双页" : "单页"}
        </button>
      </header>

      <div className="reader-body">
        {showOutline && (
          <aside className="outline-panel">
            <OutlineTree nodes={outline} onGoto={gotoPage} current={page} />
          </aside>
        )}
        <div
          className="page-container"
          ref={containerRef}
          onDoubleClick={onDoubleClick}
          onMouseDown={onContainerMouseDown}
          onClick={onContainerClick}
        >
          <div className="page-stage">
            {/* lang="en"：html 是 zh-CN，会让 span 的通用 sans-serif 解析成中文字体
                （拉丁字母比 PDF.js 测量用的 Arial 宽 ~12%），导致选词层横向漂移选错词 */}
            <div className="page-wrap">
              <canvas ref={canvas1Ref} />
              <div className="textLayer" lang="en" ref={text1Ref} />
            </div>
            <div className="page-wrap" ref={wrap2Ref} style={{ display: "none" }}>
              <canvas ref={canvas2Ref} />
              <div className="textLayer" lang="en" ref={text2Ref} />
            </div>
          </div>
          {!doc && <div className="loading-hint">正在打开…</div>}
          {toast && <div className="reader-toast">{toast}</div>}
        </div>
      </div>
      {popup && (
        <WordPopup
          result={popup.result}
          anchor={popup.anchor}
          onClose={() => setPopup(null)}
        />
      )}
    </div>
  );
}

function PasswordPrompt({
  title,
  error,
  onSubmit,
  onCancel,
}: {
  title: string;
  error: string;
  onSubmit: (pwd: string) => void;
  onCancel: () => void;
}) {
  const [pwd, setPwd] = useState("");
  return (
    <div className="reader-fallback">
      <h3>《{title}》已加密</h3>
      <p>请输入打开密码：</p>
      <input
        type="password"
        autoFocus
        value={pwd}
        onChange={(e) => setPwd(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && pwd) onSubmit(pwd);
          if (e.key === "Escape") onCancel();
        }}
      />
      {error && <p className="error-text">{error}</p>}
      <div className="modal-actions">
        <button className="btn" onClick={onCancel}>
          返回书库
        </button>
        <button className="btn primary" disabled={!pwd} onClick={() => onSubmit(pwd)}>
          打开
        </button>
      </div>
    </div>
  );
}

function OutlineTree({
  nodes,
  onGoto,
  current,
  depth = 0,
}: {
  nodes: OutlineNode[];
  onGoto: (p: number) => void;
  current: number;
  depth?: number;
}) {
  return (
    <ul className="outline-list" style={{ paddingLeft: depth === 0 ? 0 : 14 }}>
      {nodes.map((n, i) => (
        <li key={i}>
          <button
            className={"outline-item" + (n.page === current ? " active" : "")}
            disabled={n.page === null}
            onClick={() => n.page !== null && onGoto(n.page)}
          >
            {n.title}
          </button>
          {n.children.length > 0 && (
            <OutlineTree nodes={n.children} onGoto={onGoto} current={current} depth={depth + 1} />
          )}
        </li>
      ))}
    </ul>
  );
}

/** 前 1–2 页文字层字符数近似为 0 → 判定为扫描版。误判成本仅为提示文案，不追求 100% 准确。 */
async function detectScanned(doc: PDFDocumentProxy): Promise<boolean> {
  try {
    let chars = 0;
    for (let p = 1; p <= Math.min(2, doc.numPages); p++) {
      const page = await doc.getPage(p);
      const tc = await page.getTextContent();
      for (const item of tc.items) {
        if ("str" in item) chars += item.str.trim().length;
      }
      if (chars >= 20) return false;
    }
    return true;
  } catch {
    return false;
  }
}

async function loadOutline(doc: PDFDocumentProxy): Promise<OutlineNode[]> {
  interface RawItem {
    title: string;
    dest: string | unknown[] | null;
    items: RawItem[];
  }
  async function convert(items: RawItem[]): Promise<OutlineNode[]> {
    const out: OutlineNode[] = [];
    for (const it of items) {
      let pageNum: number | null = null;
      try {
        let dest = it.dest;
        if (typeof dest === "string") dest = await doc.getDestination(dest);
        if (Array.isArray(dest) && dest[0]) {
          pageNum = (await doc.getPageIndex(dest[0] as Parameters<typeof doc.getPageIndex>[0])) + 1;
        }
      } catch {
        /* 无法解析的目录项显示为灰色不可点 */
      }
      out.push({
        title: it.title,
        page: pageNum,
        children: it.items?.length ? await convert(it.items) : [],
      });
    }
    return out;
  }
  try {
    const raw = (await doc.getOutline()) as RawItem[] | null;
    return raw ? await convert(raw) : [];
  } catch {
    return [];
  }
}
