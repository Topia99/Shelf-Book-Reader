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
import { isTouchDevice } from "../platform";
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
  const [moreOpen, setMoreOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
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

  // 点击任意处关闭「更多」菜单（触发按钮上有 stopPropagation）
  useEffect(() => {
    if (!moreOpen) return;
    const close = () => setMoreOpen(false);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [moreOpen]);

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

  function openTouchMenu(book: Book, anchor: HTMLElement) {
    const rect = anchor.getBoundingClientRect();
    setCtxMenu({ x: rect.right, y: rect.bottom, book });
  }

  return (
    <div className={"library" + (dragOver ? " drag-over" : "")}>
      {/* Apple Books 式头部：大标题 + 两个小圆钮，其余控件全部收进「⋯」菜单 */}
      <header className="library-header">
        <h1 className="page-title">书库</h1>
        <div className="header-actions">
          <button
            className="icon-btn"
            aria-label="添加书籍"
            title="添加书籍"
            onClick={handleAddClick}
            disabled={busy}
          >
            <svg viewBox="0 0 20 20" aria-hidden="true">
              <path
                d="M10 4.5v11M4.5 10h11"
                stroke="currentColor"
                strokeWidth="1.8"
                strokeLinecap="round"
              />
            </svg>
          </button>
          <div className="more-wrap">
            <button
              className="icon-btn"
              aria-label="更多"
              title="更多"
              onClick={(e) => {
                e.stopPropagation();
                setMoreOpen((v) => !v);
              }}
            >
              <svg viewBox="0 0 20 20" aria-hidden="true">
                <circle cx="4.6" cy="10" r="1.7" fill="currentColor" />
                <circle cx="10" cy="10" r="1.7" fill="currentColor" />
                <circle cx="15.4" cy="10" r="1.7" fill="currentColor" />
              </svg>
            </button>
            {moreOpen && (
              <div className="more-menu" onClick={(e) => e.stopPropagation()}>
                <button
                  onClick={() => {
                    setSearchOpen(true);
                    setMoreOpen(false);
                  }}
                >
                  搜索书名
                </button>
                <button
                  onClick={() => {
                    setAccountOpen(true);
                    setMoreOpen(false);
                  }}
                >
                  {syncState.signed_in ? (
                    <span className="more-account">
                      <span className="account-trigger-badge" aria-hidden="true">
                        {accountInitial}
                      </span>
                      账号与同步
                      {syncState.syncing && (
                        <span className="account-trigger-sync" aria-label="同步中" />
                      )}
                    </span>
                  ) : (
                    "登录与同步"
                  )}
                </button>
                <div className="more-sep" />
                <div className="more-group-label">排序方式</div>
                {(Object.keys(SORT_LABELS) as SortKey[]).map((k) => (
                  <button
                    key={k}
                    className={sort === k ? "checked" : ""}
                    onClick={() => {
                      setSort(k);
                      setMoreOpen(false);
                    }}
                  >
                    {SORT_LABELS[k]}
                    {sort === k && <span className="check">✓</span>}
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      </header>

      {/* 搜索行：从「⋯ → 搜索书名」展开，取消即收起并清空 */}
      {searchOpen && (
        <div className="search-row">
          <div className="search-wrap">
            <svg className="search-icon" viewBox="0 0 20 20" aria-hidden="true">
              <circle cx="9" cy="9" r="5.5" fill="none" stroke="currentColor" strokeWidth="1.6" />
              <line
                x1="13.2"
                y1="13.2"
                x2="17"
                y2="17"
                stroke="currentColor"
                strokeWidth="1.6"
                strokeLinecap="round"
              />
            </svg>
            <input
              className="search-box"
              type="search"
              placeholder="搜索书名…"
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
          <button
            className="search-cancel"
            onClick={() => {
              setSearchOpen(false);
              setQuery("");
            }}
          >
            取消
          </button>
        </div>
      )}

      {books.length === 0 ? (
        <div className="empty-hint">
          {/* 书架插画：灰调为主、一本赭色点缀（Shelf 特色色只做提亮） */}
          <svg className="empty-illustration" viewBox="0 0 140 104" aria-hidden="true">
            <rect x="14" y="30" width="14" height="56" rx="2" fill="#D6D6DB" />
            <rect x="32" y="22" width="16" height="64" rx="2" fill="#C2C2C9" />
            <rect x="52" y="34" width="13" height="52" rx="2" fill="#B45327" opacity="0.9" />
            <g transform="rotate(14 96 62)">
              <rect x="88" y="30" width="15" height="58" rx="2" fill="#E4E4E9" />
              <line
                x1="95.5"
                y1="38"
                x2="95.5"
                y2="44"
                stroke="#fff"
                strokeWidth="2"
                strokeLinecap="round"
              />
            </g>
            <line
              x1="6"
              y1="88"
              x2="134"
              y2="88"
              stroke="#1D1D1F"
              strokeWidth="3"
              strokeLinecap="round"
              opacity="0.5"
            />
          </svg>
          <p className="empty-title">书库还是空的</p>
          <p className="empty-sub">把 PDF 拖进窗口，或从文件里挑一本开始</p>
          <button className="btn primary empty-action" onClick={handleAddClick} disabled={busy}>
            {busy ? "导入中…" : "＋ 添加第一本书"}
          </button>
        </div>
      ) : (
        <div className="book-grid">
          {books.map((book) => (
            <div
              key={book.id}
              className="book-card"
              onClick={() => {
                if (isTouchDevice) tryOpenBook(book);
              }}
              onDoubleClick={() => {
                if (!isTouchDevice) tryOpenBook(book);
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                setCtxMenu({ x: e.clientX, y: e.clientY, book });
              }}
              title={isTouchDevice ? `${book.title}（点按打开）` : `${book.title}（双击打开）`}
            >
              <BookCover book={book} />
              {/* Apple Books 式元信息行：进度居左，云状态与管理按钮居右 */}
              <div className="book-meta">
                <span className="book-meta-progress">{progressText(book)}</span>
                <span className="book-meta-icons">
                  {book.cloud_state === "remote" && (
                    <svg className="meta-cloud" viewBox="0 0 22 16" aria-label="云端待下载">
                      <path
                        d="M6.2 13.5h9.6a3.7 3.7 0 0 0 .7-7.33A5.5 5.5 0 0 0 5.7 6.9a3.3 3.3 0 0 0 .5 6.6Z"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="1.5"
                        strokeLinejoin="round"
                      />
                    </svg>
                  )}
                  <button
                    className="meta-more"
                    type="button"
                    aria-label={`管理《${book.title}》`}
                    onClick={(e) => {
                      e.stopPropagation();
                      openTouchMenu(book, e.currentTarget);
                    }}
                  >
                    <svg viewBox="0 0 16 4" aria-hidden="true">
                      <circle cx="2" cy="2" r="1.6" fill="currentColor" />
                      <circle cx="8" cy="2" r="1.6" fill="currentColor" />
                      <circle cx="14" cy="2" r="1.6" fill="currentColor" />
                    </svg>
                  </button>
                </span>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* 审计清单次要项：拖放提示文案去掉“鼠标”假设 */}
      {dragOver && <div className="drop-overlay">松开以添加 PDF 到书库</div>}

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

/** Apple Books 式进度文案：未读 / 百分比 / 读完 */
function progressText(book: Book): string {
  if (!book.total_pages || !book.last_opened_at) return "未读";
  const pct = Math.round((book.current_page / book.total_pages) * 100);
  if (pct >= 100) return "已读完";
  return `${Math.max(1, pct)}%`;
}
