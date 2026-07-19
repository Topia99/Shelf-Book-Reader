import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { saveCover, setTotalPages, updateProgress, type Book } from "../../api";
import { cleanWord, localDictSource, type LookupResult } from "../../dict";
import { isModKey, isTouchDevice } from "../../platform";
import { isPasswordError, openPdf, pdfjs, renderCoverPng, type PDFDocumentProxy } from "../../pdf";
import type { PopupAnchor } from "../WordPopup";

export type ZoomMode = "fit-width" | "fit-page" | "custom";

export interface OutlineNode {
  title: string;
  page: number | null;
  children: OutlineNode[];
}

// WebKit 专有 gesture 事件（TS 无内置类型），继承 Event 以便从监听器参数直接窄化
interface WebKitGestureEvent extends Event {
  scale: number;
}

type CaretRangeDocument = Document & {
  caretRangeFromPoint?: (x: number, y: number) => Range | null;
  caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null;
};

interface ExpandedWordRange {
  word: string;
  range: Range;
}

interface TouchTrack {
  pointerId: number;
  startX: number;
  startY: number;
  lastX: number;
  lastY: number;
  longPressTriggered: boolean;
  textTarget: boolean;
}

interface UsePdfReaderControllerArgs {
  book: Book;
  defaultZoom?: ZoomMode;
  onBack: () => void;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export function usePdfReaderController({
  book,
  defaultZoom = "fit-page",
  onBack,
}: UsePdfReaderControllerArgs) {
  const [doc, setDoc] = useState<PDFDocumentProxy | null>(null);
  const [numPages, setNumPages] = useState(0);
  const [page, setPage] = useState(1);
  const [zoom, setZoom] = useState<ZoomMode>(defaultZoom);
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
  const [toolbarHidden, setToolbarHidden] = useState(false);

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
  const gestureActiveRef = useRef(false);
  const activeTouchPointersRef = useRef(new Set<number>());
  const touchTrackRef = useRef<TouchTrack | null>(null);
  const longPressTimerRef = useRef<number | null>(null);
  const step = twoPage ? 2 : 1;

  popupRef.current = popup;

  function clearLongPressTimer() {
    if (longPressTimerRef.current !== null) {
      window.clearTimeout(longPressTimerRef.current);
      longPressTimerRef.current = null;
    }
  }

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
        updateProgress(book.id, startPage).catch(() => {});
        if (book.total_pages !== d.numPages) {
          setTotalPages(book.id, d.numPages).catch(() => {});
        }
        if (!book.cover_path) {
          renderCoverPng(d)
            .then((png) => saveCover(book.hash, png))
            .catch(() => {});
        }
        loadOutline(d).then(setOutline);
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

  useEffect(() => {
    setZoom(defaultZoom);
  }, [book.id, defaultZoom]);

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
      clearLongPressTimer();
      flushProgress();
    };
  }, [flushProgress]);

  const goBack = useCallback(() => {
    flushProgress();
    onBack();
  }, [flushProgress, onBack]);

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
      const container = el as HTMLDivElement;
      const atBottom = container.scrollTop + container.clientHeight >= container.scrollHeight - 1;
      const atTop = container.scrollTop <= 0;
      const now = Date.now();
      if ((e.deltaY > 0 && atBottom) || (e.deltaY < 0 && atTop)) {
        e.preventDefault();
        if (now - lastFlipAtRef.current < 300) return;
        lastFlipAtRef.current = now;
        if (e.deltaY > 0) next();
        else prev();
      }
    }
    function onGestureStart(e: Event) {
      const gestureEvent = e as WebKitGestureEvent;
      gestureActiveRef.current = true;
      clearLongPressTimer();
      touchTrackRef.current = null;
      gestureStartScaleRef.current = lastScaleRef.current;
      gestureEvent.preventDefault();
    }
    function onGestureChange(e: Event) {
      const gestureEvent = e as WebKitGestureEvent;
      gestureActiveRef.current = true;
      setCustomScale(clamp(gestureStartScaleRef.current * gestureEvent.scale, 0.2, 6));
      setZoom("custom");
      gestureEvent.preventDefault();
    }
    function onGestureEnd(e: Event) {
      gestureActiveRef.current = false;
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

  useEffect(() => {
    setPopup(null);
  }, [page, zoom, customScale, twoPage]);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const onScroll = () => {
      if (popupRef.current) setPopup(null);
    };
    el.addEventListener("scroll", onScroll);
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(""), 3000);
    return () => clearTimeout(t);
  }, [toast]);

  const lookupWordAtAnchor = useCallback(async (word: string, anchor: PopupAnchor) => {
    if (isScannedRef.current) {
      if (!scanToastShownRef.current) {
        scanToastShownRef.current = true;
        setToast("本书为扫描版，暂不支持取词");
      }
      return;
    }
    if (!word) return;
    try {
      const result = await localDictSource.lookup(word);
      setPopup({ result, anchor });
    } catch {
      setToast("查词失败，请重试");
    }
  }, []);

  const lookupSelectionWord = useCallback(async () => {
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
    await lookupWordAtAnchor(word, {
      left: rect.left,
      right: rect.right,
      top: rect.top,
      bottom: rect.bottom,
    });
  }, [lookupWordAtAnchor]);

  const lookupWordAtPoint = useCallback(
    async (x: number, y: number) => {
      const range = getCaretRangeFromPoint(document, x, y);
      const expanded = range ? expandWordRange(range) : null;
      if (!expanded) return;
      const rect = getRangeRect(expanded.range);
      if (!rect) return;
      await lookupWordAtAnchor(expanded.word, {
        left: rect.left,
        right: rect.right,
        top: rect.top,
        bottom: rect.bottom,
      });
    },
    [lookupWordAtAnchor]
  );

  const onContainerMouseDown = useCallback(() => {
    if (popupRef.current) {
      setPopup(null);
      popupJustClosedRef.current = true;
    }
  }, []);

  const onContainerClick = useCallback(
    (e: ReactMouseEvent<HTMLDivElement>) => {
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

  const handleTouchPointerDown = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      if (!isTouchDevice || e.pointerType !== "touch") return;
      activeTouchPointersRef.current.add(e.pointerId);
      if (activeTouchPointersRef.current.size > 1 || gestureActiveRef.current) {
        clearLongPressTimer();
        touchTrackRef.current = null;
        return;
      }
      if (popupRef.current) {
        setPopup(null);
        popupJustClosedRef.current = true;
      }
      const textTarget = Boolean((e.target as HTMLElement).closest(".textLayer"));
      touchTrackRef.current = {
        pointerId: e.pointerId,
        startX: e.clientX,
        startY: e.clientY,
        lastX: e.clientX,
        lastY: e.clientY,
        longPressTriggered: false,
        textTarget,
      };
      clearLongPressTimer();
      if (!textTarget) return;
      longPressTimerRef.current = window.setTimeout(() => {
        const track = touchTrackRef.current;
        if (!track || track.pointerId !== e.pointerId) return;
        if (gestureActiveRef.current || activeTouchPointersRef.current.size !== 1) return;
        const movedX = Math.abs(track.lastX - track.startX);
        const movedY = Math.abs(track.lastY - track.startY);
        if (movedX >= 10 || movedY >= 10) return;
        track.longPressTriggered = true;
        popupJustClosedRef.current = false;
        void lookupWordAtPoint(track.lastX, track.lastY);
      }, 500);
    },
    [lookupWordAtPoint]
  );

  const handleTouchPointerMove = useCallback((e: ReactPointerEvent<HTMLDivElement>) => {
    if (!isTouchDevice || e.pointerType !== "touch") return;
    const track = touchTrackRef.current;
    if (!track || track.pointerId !== e.pointerId) return;
    track.lastX = e.clientX;
    track.lastY = e.clientY;
    if (
      Math.abs(track.lastX - track.startX) >= 10 ||
      Math.abs(track.lastY - track.startY) >= 10
    ) {
      clearLongPressTimer();
    }
  }, []);

  const finishTouchPointer = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      if (!isTouchDevice || e.pointerType !== "touch") return;
      activeTouchPointersRef.current.delete(e.pointerId);
      const track = touchTrackRef.current;
      clearLongPressTimer();
      if (!track || track.pointerId !== e.pointerId) return;
      touchTrackRef.current = null;
      if (gestureActiveRef.current || track.longPressTriggered) {
        popupJustClosedRef.current = false;
        return;
      }
      const dx = e.clientX - track.startX;
      const dy = e.clientY - track.startY;
      const absX = Math.abs(dx);
      const absY = Math.abs(dy);
      const el = containerRef.current;
      if (!el) return;
      if (absX > 60 && absX > absY && canFlipFromHorizontalSwipe(el, dx)) {
        popupJustClosedRef.current = false;
        if (dx < 0) next();
        else prev();
        return;
      }
      if (absX >= 10 || absY >= 10) return;
      if (popupJustClosedRef.current) {
        popupJustClosedRef.current = false;
        return;
      }
      const rect = el.getBoundingClientRect();
      const rel = (e.clientX - rect.left) / rect.width;
      if (rel < 0.25) prev();
      else if (rel > 0.75) next();
      else setToolbarHidden((hidden) => !hidden);
    },
    [next, prev]
  );

  const handleTouchPointerCancel = useCallback((e: ReactPointerEvent<HTMLDivElement>) => {
    if (!isTouchDevice || e.pointerType !== "touch") return;
    activeTouchPointersRef.current.delete(e.pointerId);
    clearLongPressTimer();
    popupJustClosedRef.current = false;
    if (touchTrackRef.current?.pointerId === e.pointerId) {
      touchTrackRef.current = null;
    }
  }, []);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      setContainerSize({ w: el.clientWidth, h: el.clientHeight });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    if (!doc || !containerRef.current) return;
    let cancelled = false;
    const tasks: { cancel: () => void }[] = [];

    (async () => {
      const pageNums = twoPage && page + 1 <= numPages ? [page, page + 1] : [page];
      const proxies = await Promise.all(pageNums.map((p) => doc.getPage(p)));
      if (cancelled) return;

      const el = containerRef.current;
      if (!el) return;
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
          return;
        }
        if (cancelled) return;
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
          return;
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

  return {
    canvas1Ref,
    canvas2Ref,
    containerRef,
    customScale,
    displayScale,
    doc,
    gotoPage,
    goBack,
    handleTouchPointerCancel,
    handleTouchPointerDown,
    handleTouchPointerMove,
    loadDoc,
    loadError,
    lookupSelectionWord,
    needPassword,
    numPages,
    onContainerClick,
    onContainerMouseDown,
    outline,
    page,
    pageInput,
    passwordError,
    popup,
    setPageInput,
    setPopup,
    setShowOutline,
    setTwoPage,
    setZoom,
    showOutline,
    submitPageInput,
    text1Ref,
    text2Ref,
    toast,
    toolbarHidden,
    twoPage,
    wrap2Ref,
    zoom,
    zoomBy,
    finishTouchPointer,
  };
}

export type ReaderController = ReturnType<typeof usePdfReaderController>;

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

function getCaretRangeFromPoint(doc: Document, x: number, y: number): Range | null {
  const caretDoc = doc as CaretRangeDocument;
  if (typeof caretDoc.caretRangeFromPoint === "function") {
    return caretDoc.caretRangeFromPoint(x, y);
  }
  const caretPosition = caretDoc.caretPositionFromPoint?.(x, y);
  if (!caretPosition) return null;
  const range = doc.createRange();
  range.setStart(caretPosition.offsetNode, caretPosition.offset);
  range.collapse(true);
  return range;
}

function expandWordRange(range: Range): ExpandedWordRange | null {
  const textPoint = resolveTextPoint(range.startContainer, range.startOffset);
  if (!textPoint) return null;
  const { node, offset } = textPoint;
  const text = node.textContent ?? "";
  if (!text) return null;
  const clampedOffset = clamp(offset, 0, text.length);
  let start = clampedOffset;
  let end = clampedOffset;
  if (clampedOffset < text.length && isAsciiLetter(text[clampedOffset])) {
    end = clampedOffset + 1;
  } else if (clampedOffset > 0 && isAsciiLetter(text[clampedOffset - 1])) {
    start = clampedOffset - 1;
  } else {
    return null;
  }
  while (start > 0 && isAsciiLetter(text[start - 1])) start -= 1;
  while (end < text.length && isAsciiLetter(text[end])) end += 1;
  const word = cleanWord(text.slice(start, end));
  if (!word) return null;
  const expanded = range.cloneRange();
  expanded.setStart(node, start);
  expanded.setEnd(node, end);
  return { word, range: expanded };
}

function resolveTextPoint(container: Node, offset: number): { node: Text; offset: number } | null {
  if (container.nodeType === Node.TEXT_NODE) {
    return { node: container as Text, offset };
  }
  const children = container.childNodes;
  const nextNode = children.item(offset);
  const prevNode = children.item(Math.max(0, offset - 1));
  const nextTextNode = findTextNode(nextNode, true);
  if (nextTextNode) return { node: nextTextNode, offset: 0 };
  const prevTextNode = findTextNode(prevNode, false);
  if (!prevTextNode) return null;
  return { node: prevTextNode, offset: prevTextNode.textContent?.length ?? 0 };
}

function findTextNode(node: Node | null, forward: boolean): Text | null {
  if (!node) return null;
  if (node.nodeType === Node.TEXT_NODE) return node as Text;
  const children = Array.from(node.childNodes);
  const ordered = forward ? children : children.reverse();
  for (const child of ordered) {
    const textNode = findTextNode(child, forward);
    if (textNode) return textNode;
  }
  return null;
}

function isAsciiLetter(char: string): boolean {
  return /^[A-Za-z]$/.test(char);
}

function getRangeRect(range: Range): DOMRect | null {
  const rect = range.getBoundingClientRect();
  if (rect.width > 0 || rect.height > 0) return rect;
  const firstRect = range.getClientRects()[0];
  return firstRect ?? null;
}

function canFlipFromHorizontalSwipe(container: HTMLDivElement, dx: number): boolean {
  const maxScrollLeft = container.scrollWidth - container.clientWidth;
  if (maxScrollLeft <= 1) return true;
  if (dx < 0) return container.scrollLeft >= maxScrollLeft - 1;
  return container.scrollLeft <= 1;
}
