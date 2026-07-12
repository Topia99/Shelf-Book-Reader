import { useCallback, useEffect, useRef, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  addBooks,
  listBooks,
  removeBook,
  renameBook,
  saveCover,
  setTotalPages,
  syncStatus,
  type Book,
  type SortKey,
  type SyncStatus,
} from "../api";
import { isPasswordError, openPdf, renderCoverPng } from "../pdf";
import AccountPanel from "./AccountPanel";
import BookCover from "./BookCover";

interface Props {
  onOpenBook: (book: Book) => void;
}

interface Notice {
  kind: "info" | "error";
  text: string;
}

interface CtxMenu {
  x: number;
  y: number;
  book: Book;
}

const SORT_LABELS: Record<SortKey, string> = {
  recent: "最近阅读",
  added: "添加时间",
  title: "书名",
};

export default function Library({ onOpenBook }: Props) {
  /** 云端书籍本机还没有文件本体（文件同步在后续版本），拦截打开并提示 */
  function tryOpenBook(book: Book) {
    if (book.cloud_state === "remote") {
      pushNotice(
        "info",
        `《${book.title}》的文件在云端，尚未下载到本机（书目与进度已同步，文件同步功能即将上线）`
      );
      return;
    }
    onOpenBook(book);
  }
  const [books, setBooks] = useState<Book[]>([]);
  const [sort, setSort] = useState<SortKey>("recent");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [notices, setNotices] = useState<Notice[]>([]);
  const [ctxMenu, setCtxMenu] = useState<CtxMenu | null>(null);
  const [renaming, setRenaming] = useState<Book | null>(null);
  const [confirmRemove, setConfirmRemove] = useState<Book | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [accountOpen, setAccountOpen] = useState(false);
  const [syncState, setSyncState] = useState<SyncStatus>({
    signed_in: false,
    email: null,
    syncing: false,
    last_sync_ms: null,
    last_error: null,
  });
  const renameInputRef = useRef<HTMLInputElement>(null);

  const reload = useCallback(async () => {
    setBooks(await listBooks(sort, query));
  }, [sort, query]);

  useEffect(() => {
    reload().catch((e) => pushNotice("error", String(e)));
  }, [reload]);

  const refreshSyncStatus = useCallback(async () => {
    setSyncState(await syncStatus());
  }, []);

  function pushNotice(kind: Notice["kind"], text: string) {
    setNotices((ns) => [...ns, { kind, text }]);
    // 5 秒后自动消失
    setTimeout(() => setNotices((ns) => ns.slice(1)), 5000);
  }

  /** 入库后处理：提取元数据书名、总页数、封面。加密/损坏的 PDF 静默降级为默认封面。 */
  const postProcessBook = useCallback(async (book: Book) => {
    try {
      const doc = await openPdf(book.file_path).promise;
      try {
        await setTotalPages(book.id, doc.numPages);
        try {
          const meta = (await doc.getMetadata()) as { info?: { Title?: string } };
          const metaTitle = meta.info?.Title?.trim();
          if (metaTitle) {
            await renameBook(book.id, metaTitle);
          }
        } catch {
          /* 元数据读取失败不影响入库 */
        }
        const png = await renderCoverPng(doc);
        await saveCover(book.hash, png);
      } finally {
        await doc.destroy();
      }
    } catch (e) {
      if (isPasswordError(e)) {
        pushNotice("info", `《${book.title}》是加密 PDF，已使用默认封面，打开时需输入密码`);
      } else {
        pushNotice("info", `《${book.title}》无法渲染封面，已使用默认封面`);
      }
    }
  }, []);

  const importPaths = useCallback(
    async (paths: string[]) => {
      const pdfPaths = paths.filter((p) => p.toLowerCase().endsWith(".pdf"));
      if (pdfPaths.length === 0) {
        pushNotice("error", "没有可导入的 PDF 文件");
        return;
      }
      setBusy(true);
      try {
        const results = await addBooks(pdfPaths);
        for (const r of results) {
          if (r.status === "duplicate") {
            pushNotice("info", `跳过重复：${r.message ?? r.path}`);
          } else if (r.status === "error") {
            pushNotice("error", `导入失败（${r.path}）：${r.message ?? "未知错误"}`);
          }
        }
        const added = results.filter((r) => r.status === "added" && r.book);
        if (added.length > 0) {
          pushNotice("info", `已添加 ${added.length} 本书`);
        }
        await reload();
        // 封面/页数提取在后台逐本进行，完成后刷新
        for (const r of added) {
          await postProcessBook(r.book!);
        }
        await reload();
      } finally {
        setBusy(false);
      }
    },
    [reload, postProcessBook]
  );

  // 拖拽 PDF 入库
  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "enter" || event.payload.type === "over") {
        setDragOver(true);
      } else if (event.payload.type === "drop") {
        setDragOver(false);
        importPaths(event.payload.paths);
      } else {
        setDragOver(false);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [importPaths]);

  // 点击任意处关闭右键菜单
  useEffect(() => {
    if (!ctxMenu) return;
    const close = () => setCtxMenu(null);
    window.addEventListener("click", close);
    window.addEventListener("contextmenu", close, { capture: true, once: true });
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("contextmenu", close, { capture: true });
    };
  }, [ctxMenu]);

  useEffect(() => {
    if (renaming) renameInputRef.current?.select();
  }, [renaming]);

  // 账号状态初始化 + 持续订阅 Rust 侧同步事件
  useEffect(() => {
    refreshSyncStatus().catch((e) => pushNotice("error", String(e)));

    let cancelled = false;
    const unlistenPromise = listen<SyncStatus>("sync-status", (event) => {
      if (!cancelled) {
        setSyncState(event.payload);
      }
    });

    return () => {
      cancelled = true;
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [refreshSyncStatus]);

  const accountInitial = syncState.email?.trim().charAt(0).toUpperCase() || "账";

  async function handleAddClick() {
    const selected = await openFileDialog({
      multiple: true,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      title: "选择要添加的 PDF",
    });
    if (!selected) return;
    await importPaths(Array.isArray(selected) ? selected : [selected]);
  }

  async function handleRemove(book: Book) {
    setConfirmRemove(null);
    try {
      await removeBook(book.id);
      await reload();
      pushNotice("info", `已从书库移除《${book.title}》`);
    } catch (e) {
      pushNotice("error", String(e));
    }
  }

  async function handleRenameSubmit(book: Book, title: string) {
    setRenaming(null);
    if (!title.trim() || title.trim() === book.title) return;
    try {
      await renameBook(book.id, title.trim());
      await reload();
    } catch (e) {
      pushNotice("error", String(e));
    }
  }

  return (
    <div className={"library" + (dragOver ? " drag-over" : "")}>
      <header className="library-toolbar">
        <h1 className="app-title">Shelf</h1>
        <input
          className="search-box"
          type="search"
          placeholder="搜索书名…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button className="account-trigger" onClick={() => setAccountOpen(true)}>
          {syncState.signed_in ? (
            <>
              <span className="account-trigger-badge" aria-hidden="true">
                {accountInitial}
              </span>
              <span className="account-trigger-email">{syncState.email}</span>
              {syncState.syncing && <span className="account-trigger-sync" aria-label="同步中" />}
            </>
          ) : (
            "登录"
          )}
        </button>
        <select
          className="sort-select"
          value={sort}
          onChange={(e) => setSort(e.target.value as SortKey)}
        >
          {(Object.keys(SORT_LABELS) as SortKey[]).map((k) => (
            <option key={k} value={k}>
              {SORT_LABELS[k]}
            </option>
          ))}
        </select>
        <button className="btn primary" onClick={handleAddClick} disabled={busy}>
          {busy ? "导入中…" : "＋ 添加书籍"}
        </button>
      </header>

      {books.length === 0 ? (
        <div className="empty-hint">
          <p>书库还是空的</p>
          <p>点击「添加书籍」或把 PDF 拖到窗口里</p>
        </div>
      ) : (
        <div className="book-grid">
          {books.map((book) => (
            <div
              key={book.id}
              className="book-card"
              onDoubleClick={() => tryOpenBook(book)}
              onContextMenu={(e) => {
                e.preventDefault();
                setCtxMenu({ x: e.clientX, y: e.clientY, book });
              }}
              title={`${book.title}（双击打开）`}
            >
              <BookCover book={book} />
              {book.cloud_state === "remote" && <span className="cloud-badge">云端</span>}
              <div className="book-title">{book.title}</div>
              <div className="book-progress">{progressText(book)}</div>
            </div>
          ))}
        </div>
      )}

      {dragOver && <div className="drop-overlay">松开鼠标，添加 PDF 到书库</div>}

      {ctxMenu && (
        <div className="context-menu" style={{ left: ctxMenu.x, top: ctxMenu.y }}>
          <button onClick={() => tryOpenBook(ctxMenu.book)}>打开</button>
          <button onClick={() => setRenaming(ctxMenu.book)}>重命名</button>
          <button className="danger" onClick={() => setConfirmRemove(ctxMenu.book)}>
            从书库移除
          </button>
        </div>
      )}

      {renaming && (
        <div className="modal-backdrop" onClick={() => setRenaming(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>重命名</h3>
            <input
              ref={renameInputRef}
              defaultValue={renaming.title}
              onKeyDown={(e) => {
                if (e.key === "Enter")
                  handleRenameSubmit(renaming, (e.target as HTMLInputElement).value);
                if (e.key === "Escape") setRenaming(null);
              }}
            />
            <div className="modal-actions">
              <button className="btn" onClick={() => setRenaming(null)}>
                取消
              </button>
              <button
                className="btn primary"
                onClick={() => handleRenameSubmit(renaming, renameInputRef.current?.value ?? "")}
              >
                确定
              </button>
            </div>
          </div>
        </div>
      )}

      {confirmRemove && (
        <div className="modal-backdrop" onClick={() => setConfirmRemove(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>从书库移除</h3>
            <p>
              确定移除《{confirmRemove.title}》？将删除书库内的副本和封面缓存，不影响你的源文件。
            </p>
            <div className="modal-actions">
              <button className="btn" onClick={() => setConfirmRemove(null)}>
                取消
              </button>
              <button className="btn danger" onClick={() => handleRemove(confirmRemove)}>
                移除
              </button>
            </div>
          </div>
        </div>
      )}

      <AccountPanel
        open={accountOpen}
        status={syncState}
        onClose={() => setAccountOpen(false)}
        onRefreshStatus={refreshSyncStatus}
      />

      <div className="notices">
        {notices.map((n, i) => (
          <div key={i} className={`notice ${n.kind}`}>
            {n.text}
          </div>
        ))}
      </div>
    </div>
  );
}

function progressText(book: Book): string {
  if (!book.total_pages) return "未读";
  if (!book.last_opened_at) return `共 ${book.total_pages} 页`;
  const pct = Math.round((book.current_page / book.total_pages) * 100);
  return `${book.current_page}/${book.total_pages} · ${pct}%`;
}
