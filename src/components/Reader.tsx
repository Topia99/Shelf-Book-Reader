import { useCallback, useEffect, useRef, useState } from "react";
import { saveCover, setTotalPages, updateProgress, type Book } from "../api";
import { isPasswordError, openPdf, renderCoverPng, type PDFDocumentProxy } from "../pdf";

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

  const containerRef = useRef<HTMLDivElement>(null);
  const docRef = useRef<PDFDocumentProxy | null>(null);
  const canvas1Ref = useRef<HTMLCanvasElement>(null);
  const canvas2Ref = useRef<HTMLCanvasElement>(null);
  const lastScaleRef = useRef(1);
  const pageRef = useRef(1);
  const lastFlipAtRef = useRef(0);
  const step = twoPage ? 2 : 1;

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
          goBack();
          break;
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [next, prev, gotoPage, numPages, goBack]);

  // 滚轮：Ctrl+滚轮缩放；普通滚轮先滚动页面内容，到边缘再翻页
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    function onWheel(e: WheelEvent) {
      if (e.ctrlKey) {
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
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [next, prev]);

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

      const dpr = window.devicePixelRatio || 1;
      const canvases = [canvas1Ref.current, canvas2Ref.current];
      canvases[1]!.style.display = proxies.length > 1 ? "block" : "none";

      for (let i = 0; i < proxies.length; i++) {
        const canvas = canvases[i];
        if (!canvas || cancelled) return;
        const vp = proxies[i].getViewport({ scale: scale * dpr });
        canvas.width = Math.floor(vp.width);
        canvas.height = Math.floor(vp.height);
        canvas.style.width = `${Math.floor(vp.width / dpr)}px`;
        canvas.style.height = `${Math.floor(vp.height / dpr)}px`;
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
        <div className="page-container" ref={containerRef}>
          <div className="page-stage">
            <canvas ref={canvas1Ref} />
            <canvas ref={canvas2Ref} style={{ display: "none" }} />
          </div>
          <div className="nav-zone nav-left" onClick={prev} title="上一页" />
          <div className="nav-zone nav-right" onClick={next} title="下一页" />
          {!doc && <div className="loading-hint">正在打开…</div>}
        </div>
      </div>
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
