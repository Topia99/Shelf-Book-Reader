//! P3-6：同步引擎接线。
//!
//! 引擎运行在专用 `std::thread`（sync_supabase 的阻塞客户端要求），
//! 自己持有一条到 library.db 的独立连接（WAL 模式支持多连接并发），
//! 通过 mpsc 通道接收 UI 线程的命令，状态变化经 Tauri 事件 `sync-status` 推给前端。
//!
//! 触发模型：登录成功 / 显式 sync_now / 翻页（update_progress 后发 SyncNow）都会
//! 安排一次防抖后的同步；无事件时每 5 分钟心跳一次；失败按 2^n 指数退避（上限 10 分钟）。

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::Serialize;
use tauri::Emitter;

use crate::sync::{SyncBackend, SyncError};
use crate::sync_engine;
use crate::sync_supabase::SupabaseBackend;
use crate::token_store;

/// 云端默认配置。anon key 是公开的客户端凭证（安全边界在 RLS，不在这里）；
/// 本地栈联调用环境变量 SHELF_SUPABASE_URL / SHELF_SUPABASE_ANON_KEY 覆盖。
const DEFAULT_SUPABASE_URL: &str = "https://dyhpapzyyuxlqpqupsfo.supabase.co";
const DEFAULT_ANON_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6ImR5aHBhcHp5eXV4bHFwcXVwc2ZvIiwicm9sZSI6ImFub24iLCJpYXQiOjE3ODM4NzEyMjEsImV4cCI6MjA5OTQ0NzIyMX0.8UqryIQSWAi2gj24pudqZ_Sr4L0UZNuCMwhu_jq9BPI";

const HEARTBEAT: Duration = Duration::from_secs(300);
const DEBOUNCE: Duration = Duration::from_secs(5);
const MIN_INTERVAL: Duration = Duration::from_secs(30);
const BACKOFF_BASE_SECS: u64 = 5;
const BACKOFF_CAP_SECS: u64 = 600;
const PULL_PAGE_LIMIT: u32 = 500;
const MAX_PULL_PAGES: usize = 10;
/// 会话剩余有效期低于该值时先刷新再同步
const REFRESH_MARGIN_MS: i64 = 60_000;

fn supabase_config() -> (String, String) {
    let url = std::env::var("SHELF_SUPABASE_URL").unwrap_or_else(|_| DEFAULT_SUPABASE_URL.into());
    let key = std::env::var("SHELF_SUPABASE_ANON_KEY").unwrap_or_else(|_| DEFAULT_ANON_KEY.into());
    (url, key)
}

/// UI 线程发给引擎的命令；带 reply 的命令用一次性通道回传结果。
pub(crate) enum SyncCommand {
    SignIn {
        email: String,
        password: String,
        reply: Sender<Result<(), String>>,
    },
    SignUp {
        email: String,
        password: String,
        reply: Sender<Result<(), String>>,
    },
    SignOut {
        reply: Sender<Result<(), String>>,
    },
    DeleteAccount {
        reply: Sender<Result<(), String>>,
    },
    /// 按需下载云端书籍文件本体（用户点开 remote 书时触发）
    DownloadBook {
        hash: String,
        reply: Sender<Result<(), String>>,
    },
    /// 请求一次同步（防抖合并，可安全高频发送）
    SyncNow,
}

/// 暴露给前端的同步状态快照。
#[derive(Debug, Clone, Serialize, Default)]
pub(crate) struct SyncStatus {
    pub signed_in: bool,
    pub email: Option<String>,
    pub syncing: bool,
    pub last_sync_ms: Option<i64>,
    pub last_error: Option<String>,
}

/// 由 Tauri 托管的引擎句柄。Sender 非 Sync，用 Mutex 包一层。
pub(crate) struct SyncHandle {
    tx: Mutex<Sender<SyncCommand>>,
    status: Arc<Mutex<SyncStatus>>,
}

impl SyncHandle {
    pub(crate) fn send(&self, cmd: SyncCommand) -> Result<(), String> {
        self.tx
            .lock()
            .unwrap()
            .send(cmd)
            .map_err(|_| "同步引擎已退出".to_string())
    }

    /// 发送带回执的命令并等待结果（登录/登出等 UI 阻塞操作，30 秒超时）。
    pub(crate) fn request(
        &self,
        make: impl FnOnce(Sender<Result<(), String>>) -> SyncCommand,
    ) -> Result<(), String> {
        self.request_with_timeout(make, 30)
    }

    /// 同 request，但可指定超时（下载大文件用更长超时）。
    pub(crate) fn request_with_timeout(
        &self,
        make: impl FnOnce(Sender<Result<(), String>>) -> SyncCommand,
        timeout_secs: u64,
    ) -> Result<(), String> {
        let (reply_tx, reply_rx) = channel();
        self.send(make(reply_tx))?;
        reply_rx
            .recv_timeout(Duration::from_secs(timeout_secs))
            .map_err(|_| "同步引擎响应超时".to_string())?
    }

    pub(crate) fn status(&self) -> SyncStatus {
        self.status.lock().unwrap().clone()
    }
}

pub(crate) fn spawn(app: tauri::AppHandle, db_path: PathBuf) -> SyncHandle {
    let (tx, rx) = channel();
    let status = Arc::new(Mutex::new(SyncStatus::default()));
    let status_for_thread = status.clone();
    thread::Builder::new()
        .name("shelf-sync".into())
        .spawn(move || engine_loop(app, db_path, rx, status_for_thread))
        .expect("无法创建同步线程");
    SyncHandle {
        tx: Mutex::new(tx),
        status,
    }
}

/// 修改状态并广播给前端；UI 侧监听 `sync-status` 事件即可实时刷新。
fn update_status(
    app: &tauri::AppHandle,
    status: &Arc<Mutex<SyncStatus>>,
    f: impl FnOnce(&mut SyncStatus),
) {
    let snapshot = {
        let mut guard = status.lock().unwrap();
        f(&mut guard);
        guard.clone()
    };
    let _ = app.emit("sync-status", snapshot);
}

fn engine_loop(
    app: tauri::AppHandle,
    db_path: PathBuf,
    rx: Receiver<SyncCommand>,
    status: Arc<Mutex<SyncStatus>>,
) {
    let db = match Connection::open(&db_path) {
        Ok(db) => db,
        Err(e) => {
            update_status(&app, &status, |s| {
                s.last_error = Some(format!("同步引擎无法打开数据库：{e}"))
            });
            return;
        }
    };
    let _ = db.busy_timeout(Duration::from_secs(5));

    // 文件路径存相对 base（books/<hash>.pdf），上传时拼回绝对；base = library.db 所在目录
    let base_dir: PathBuf = db_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let (url, key) = supabase_config();
    let mut backend = SupabaseBackend::new(url, key);

    // 启动时从钥匙串恢复会话；refresh 校验有效性，失效则静默清除（用户重新登录即可）
    let loaded = token_store::load_session();
    eprintln!("[sync] 会话加载结果: {:?}", loaded.as_ref().map(|o| o.is_some()));
    if let Ok(Some(saved)) = loaded {
        backend.set_session(saved);
        match backend.refresh() {
            Ok(fresh) => {
                eprintln!("[sync] 会话恢复并刷新成功");
                let _ = token_store::save_session(&fresh);
                update_status(&app, &status, |s| s.signed_in = true);
            }
            Err(e) => {
                eprintln!("[sync] 会话刷新失败: {e}");
                let _ = token_store::clear_session();
                backend = {
                    let (url, key) = supabase_config();
                    SupabaseBackend::new(url, key)
                };
            }
        }
    }

    // pending：下一次允许执行同步的时刻（防抖 + 退避共用）；None 表示只等心跳
    let mut pending: Option<Instant> = None;
    let mut last_run: Option<Instant> = None;
    let mut failures: u32 = 0;
    if status.lock().unwrap().signed_in {
        pending = Some(Instant::now()); // 恢复会话成功后立即同步一轮
    }

    loop {
        let wait = pending
            .map(|p| p.saturating_duration_since(Instant::now()))
            .unwrap_or(HEARTBEAT)
            .min(HEARTBEAT);

        match rx.recv_timeout(wait) {
            Ok(SyncCommand::SignIn {
                email,
                password,
                reply,
            }) => {
                let result = backend
                    .sign_in(&email, &password)
                    .map_err(|e| e.to_string())
                    .map(|session| {
                        let _ = token_store::save_session(&session);
                    });
                let ok = result.is_ok();
                let _ = reply.send(result);
                update_status(&app, &status, |s| {
                    s.signed_in = ok;
                    s.email = ok.then(|| email.clone());
                    if ok {
                        s.last_error = None;
                    }
                });
                if ok {
                    failures = 0;
                    pending = Some(Instant::now());
                }
            }
            Ok(SyncCommand::SignUp {
                email,
                password,
                reply,
            }) => {
                let result = backend
                    .sign_up(&email, &password)
                    .map_err(|e| e.to_string())
                    .map(|session| {
                        let _ = token_store::save_session(&session);
                    });
                let ok = result.is_ok();
                let _ = reply.send(result);
                update_status(&app, &status, |s| {
                    s.signed_in = ok;
                    s.email = ok.then(|| email.clone());
                    if ok {
                        s.last_error = None;
                    }
                });
                if ok {
                    failures = 0;
                    pending = Some(Instant::now());
                }
            }
            Ok(SyncCommand::SignOut { reply }) => {
                let _ = backend.sign_out(); // 远端登出失败不阻塞本地登出
                let _ = token_store::clear_session();
                let _ = reply.send(Ok(()));
                pending = None;
                failures = 0;
                update_status(&app, &status, |s| {
                    s.signed_in = false;
                    s.email = None;
                    s.syncing = false;
                });
            }
            Ok(SyncCommand::DeleteAccount { reply }) => {
                let result = backend.delete_account().map_err(|e| e.to_string());
                if result.is_ok() {
                    let _ = token_store::clear_session();
                    pending = None;
                    failures = 0;
                    update_status(&app, &status, |s| {
                        s.signed_in = false;
                        s.email = None;
                        s.syncing = false;
                    });
                }
                let _ = reply.send(result);
            }
            Ok(SyncCommand::DownloadBook { hash, reply }) => {
                let result =
                    download_book(&db, &mut backend, &base_dir, &hash).map_err(|e| e.to_string());
                let _ = reply.send(result);
            }
            Ok(SyncCommand::SyncNow) => {
                if status.lock().unwrap().signed_in {
                    // 防抖：合并 5 秒内的连发；且与上次成功同步至少间隔 30 秒
                    let earliest = last_run
                        .map(|t| t + MIN_INTERVAL)
                        .unwrap_or_else(Instant::now);
                    let target = (Instant::now() + DEBOUNCE).max(earliest);
                    pending = Some(pending.map_or(target, |p| p.min(target).max(earliest)));
                }
            }
            Err(RecvTimeoutError::Disconnected) => break, // 应用退出
            Err(RecvTimeoutError::Timeout) => {
                let due = pending.is_none_or(|p| Instant::now() >= p);
                if !status.lock().unwrap().signed_in || !due {
                    continue;
                }
                update_status(&app, &status, |s| s.syncing = true);
                match run_cycle(&db, &mut backend, &base_dir) {
                    Ok(()) => {
                        eprintln!("[sync] 同步周期完成");
                        failures = 0;
                        pending = None;
                        last_run = Some(Instant::now());
                        update_status(&app, &status, |s| {
                            s.syncing = false;
                            s.last_sync_ms = Some(crate::now_ms());
                            s.last_error = None;
                        });
                    }
                    Err(e) => {
                        eprintln!("[sync] 同步周期失败: {e}");
                        failures = failures.saturating_add(1);
                        let delay = (BACKOFF_BASE_SECS << failures.min(7)).min(BACKOFF_CAP_SECS);
                        pending = Some(Instant::now() + Duration::from_secs(delay));
                        let unauthorized = matches!(e, SyncError::Unauthorized);
                        update_status(&app, &status, |s| {
                            s.syncing = false;
                            s.last_error = Some(e.to_string());
                            if unauthorized {
                                s.signed_in = false;
                                s.email = None;
                            }
                        });
                        if unauthorized {
                            let _ = token_store::clear_session();
                            pending = None;
                        }
                    }
                }
            }
        }
    }
}

/// 一轮完整同步：会话保鲜 → push 脏行 → 按游标分页 pull 合并。
/// 单次 PUT 上传的文件大小上限（MVP）。>100MB 暂不上传，留待 P5-1b 分片续传。
const MAX_UPLOAD_BYTES: u64 = 100 * 1024 * 1024;

fn run_cycle(
    db: &Connection,
    backend: &mut SupabaseBackend,
    base_dir: &Path,
) -> Result<(), SyncError> {
    let session = backend.session().ok_or(SyncError::Unauthorized)?;
    if session.expires_at - crate::now_ms() < REFRESH_MARGIN_MS {
        let fresh = backend.refresh()?;
        let _ = token_store::save_session(&fresh);
    }

    // push：同一批脏行同时产出书目与进度两组载荷
    let (books, progress) = sync_engine::collect_dirty(db).map_err(db_err)?;
    if !books.is_empty() {
        backend.push_books(&books)?;
        backend.push_progress(&progress)?;
        // synced_at 用行自身的 updated_at：推送后到现在之间的新写入仍保持脏状态
        for book in &books {
            sync_engine::mark_synced(db, std::slice::from_ref(&book.sha256), book.updated_at)
                .map_err(db_err)?;
        }
    }

    // pull：按服务器游标增量分页拉取（上限 MAX_PULL_PAGES 页，超出留给下一轮）
    let mut cursor = sync_engine::get_cursor(db).map_err(db_err)?;
    for _ in 0..MAX_PULL_PAGES {
        let page = backend.pull_since(cursor.as_deref(), PULL_PAGE_LIMIT)?;
        sync_engine::merge_remote_books(db, &page.books, crate::now_ms()).map_err(db_err)?;
        sync_engine::merge_remote_progress(db, &page.progress).map_err(db_err)?;
        match page.next_cursor {
            Some(next) => {
                sync_engine::set_cursor(db, &next).map_err(db_err)?;
                cursor = Some(next);
            }
            None => break,
        }
    }

    // 文件上传放在元数据同步之后：即使上传失败也不影响书目/进度同步。
    upload_pending_files(db, backend, base_dir)?;
    Ok(())
}

/// 上传本机待传书的文件本体（cloud_state='local'）。元数据 push 后云端行已存在，
/// 上传成功即回填 file_key 并置 synced。配额满时停止本轮并向上传播提示；
/// 单本瞬时失败回退 local、下轮重试；>100MB 暂跳过（MVP 单次 PUT）。
fn upload_pending_files(
    db: &Connection,
    backend: &SupabaseBackend,
    base_dir: &Path,
) -> Result<(), SyncError> {
    let candidates = sync_engine::collect_uploadable(db).map_err(db_err)?;
    for c in candidates {
        let abs = base_dir.join(&c.file_path);
        let Ok(meta) = std::fs::metadata(&abs) else {
            continue; // 文件不在本机（异常态），跳过
        };
        if meta.len() > MAX_UPLOAD_BYTES {
            continue; // 大文件 MVP 暂不传
        }
        let Ok(bytes) = std::fs::read(&abs) else {
            continue;
        };
        sync_engine::set_cloud_state(db, &c.hash, "uploading").map_err(db_err)?;
        match backend.upload_book_file(&c.hash, &bytes) {
            Ok(()) => {
                sync_engine::set_cloud_state(db, &c.hash, "synced").map_err(db_err)?;
            }
            Err(SyncError::QuotaExceeded) => {
                sync_engine::set_cloud_state(db, &c.hash, "local").map_err(db_err)?;
                return Err(SyncError::QuotaExceeded);
            }
            Err(_) => {
                // 瞬时失败：回退 local，本轮跳过该书，下轮重试
                sync_engine::set_cloud_state(db, &c.hash, "local").map_err(db_err)?;
            }
        }
    }
    Ok(())
}

/// 按需下载 remote 书文件：会话保鲜 → 预签名 GET → 校验 SHA-256 → 写入书库 → 置 synced。
fn download_book(
    db: &Connection,
    backend: &mut SupabaseBackend,
    base_dir: &Path,
    hash: &str,
) -> Result<(), SyncError> {
    let session = backend.session().ok_or(SyncError::Unauthorized)?;
    if session.expires_at - crate::now_ms() < REFRESH_MARGIN_MS {
        let fresh = backend.refresh()?;
        let _ = token_store::save_session(&fresh);
    }

    let rel: String = db
        .query_row(
            "SELECT file_path FROM books WHERE hash = ?1 AND deleted = 0",
            [hash],
            |r| r.get(0),
        )
        .map_err(db_err)?;

    let bytes = backend.download_book_file(hash)?;
    if crate::sha256_of_bytes(&bytes) != hash {
        return Err(SyncError::Other("下载文件校验失败（哈希不符）".into()));
    }

    let abs = base_dir.join(&rel);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| SyncError::Other(format!("创建书库目录失败：{e}")))?;
    }
    std::fs::write(&abs, &bytes).map_err(|e| SyncError::Other(format!("写入文件失败：{e}")))?;
    sync_engine::set_cloud_state(db, hash, "synced").map_err(db_err)?;
    Ok(())
}

fn db_err(e: rusqlite::Error) -> SyncError {
    SyncError::Other(format!("本地数据库错误：{e}"))
}
