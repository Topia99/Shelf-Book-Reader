mod sync;
#[allow(dead_code)] // P3-6 接线后移除
mod sync_engine;
mod sync_supabase;

use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Manager, State};

struct AppState {
    db: Mutex<Connection>,
    /// 只读词典库；缺失时为 None，查词一律返回未找到
    dict: Mutex<Option<Connection>>,
    base_dir: PathBuf,
    books_dir: PathBuf,
    covers_dir: PathBuf,
}

#[derive(Serialize, Clone)]
struct Book {
    id: i64,
    hash: String,
    title: String,
    file_path: String,
    cover_path: Option<String>,
    total_pages: Option<i64>,
    current_page: i64,
    added_at: String,
    last_opened_at: Option<String>,
}

#[derive(Serialize)]
struct AddResult {
    path: String,
    /// "added" | "duplicate" | "error"
    status: String,
    message: Option<String>,
    book: Option<Book>,
}

const BOOK_COLS: &str =
    "id, hash, title, file_path, cover_path, total_pages, current_page, added_at, last_opened_at";

fn row_to_book(row: &rusqlite::Row) -> rusqlite::Result<Book> {
    Ok(Book {
        id: row.get(0)?,
        hash: row.get(1)?,
        title: row.get(2)?,
        file_path: row.get(3)?,
        cover_path: row.get(4)?,
        total_pages: row.get(5)?,
        current_page: row.get(6)?,
        added_at: row.get(7)?,
        last_opened_at: row.get(8)?,
    })
}

/// DB 里 file_path/cover_path 存相对 base 的路径（'/' 分隔，如 "books/<hash>.pdf"），
/// 数据目录搬家（Windows 迁移、iOS 容器路径变化）不影响存量记录；
/// 返回给前端或访问文件系统前用本函数拼回绝对路径
fn to_abs(base: &Path, rel: &str) -> String {
    rel.split('/')
        .fold(base.to_path_buf(), |p, seg| p.join(seg))
        .to_string_lossy()
        .to_string()
}

fn absolutize_book(base: &Path, mut book: Book) -> Book {
    book.file_path = to_abs(base, &book.file_path);
    book.cover_path = book.cover_path.map(|c| to_abs(base, &c));
    book
}

fn sha256_of_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("无法打开文件：{e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("读取文件失败：{e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 统一使用 Unix 毫秒时间戳，供后续云同步按 updated_at 做脏数据扫描
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn get_book_by_id(db: &Connection, id: i64) -> Result<Book, String> {
    db.query_row(
        &format!("SELECT {BOOK_COLS} FROM books WHERE id = ?1"),
        [id],
        row_to_book,
    )
    .map_err(|e| format!("找不到书籍：{e}"))
}

fn revive_deleted_book_record(db: &Connection, id: i64, file_path: &str) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE books
         SET deleted = 0,
             file_path = ?2,
             updated_at = ?3,
             added_at = datetime('now', 'localtime')
         WHERE id = ?1",
        rusqlite::params![id, file_path, now_ms()],
    )?;
    Ok(())
}

fn tombstone_book(db: &Connection, id: i64) -> Result<(), String> {
    db.execute(
        "UPDATE books SET deleted = 1, updated_at = ?2 WHERE id = ?1",
        rusqlite::params![id, now_ms()],
    )
    .map_err(|e| format!("删除记录失败：{e}"))?;
    Ok(())
}

fn update_progress_in_db(db: &Connection, id: i64, page: i64) -> Result<(), String> {
    db.execute(
        "UPDATE books
         SET current_page = ?2,
             last_opened_at = datetime('now', 'localtime'),
             updated_at = ?3
         WHERE id = ?1",
        rusqlite::params![id, page, now_ms()],
    )
    .map_err(|e| format!("保存进度失败：{e}"))?;
    Ok(())
}

#[tauri::command]
fn add_books(paths: Vec<String>, state: State<AppState>) -> Vec<AddResult> {
    let mut results = Vec::new();
    for p in paths {
        results.push(add_one_book(&p, &state));
    }
    results
}

fn add_one_book(path_str: &str, state: &State<AppState>) -> AddResult {
    let err = |msg: String| AddResult {
        path: path_str.to_string(),
        status: "error".into(),
        message: Some(msg),
        book: None,
    };

    let src = Path::new(path_str);
    if !src
        .extension()
        .map(|e| e.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
    {
        return err("不是 PDF 文件".into());
    }
    if !src.is_file() {
        return err("文件不存在".into());
    }

    let hash = match sha256_of_file(src) {
        Ok(h) => h,
        Err(e) => return err(e),
    };

    let db = state.db.lock().unwrap();
    let existing = db
        .query_row(
            "SELECT id, deleted FROM books WHERE hash = ?1",
            [&hash],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .ok();

    // 查重：同哈希即同一本书（即使文件名不同）
    if let Some((id, deleted)) = existing {
        if deleted == 0 {
            let existing = match db.query_row(
                &format!("SELECT {BOOK_COLS} FROM books WHERE hash = ?1"),
                [&hash],
                row_to_book,
            ) {
                Ok(book) => book,
                Err(e) => return err(format!("读取重复记录失败：{e}")),
            };
            return AddResult {
                path: path_str.to_string(),
                status: "duplicate".into(),
                message: Some(format!("已在书库中：{}", existing.title)),
                book: Some(absolutize_book(&state.base_dir, existing)),
            };
        }

        let dest = state.books_dir.join(format!("{hash}.pdf"));
        if let Err(e) = fs::copy(src, &dest) {
            let _ = fs::remove_file(&dest); // 清理可能的半截文件（如磁盘满）
            return err(format!("复制入库失败：{e}"));
        }

        let revived =
            revive_deleted_book_record(&db, id, &format!("books/{hash}.pdf")).and_then(|_| {
                db.query_row(
                    &format!("SELECT {BOOK_COLS} FROM books WHERE id = ?1"),
                    [id],
                    row_to_book,
                )
            });

        return match revived {
            Ok(book) => AddResult {
                path: path_str.to_string(),
                status: "added".into(),
                message: None,
                book: Some(absolutize_book(&state.base_dir, book)),
            },
            Err(e) => {
                let _ = fs::remove_file(&dest);
                err(format!("复活已删除书籍失败：{e}"))
            }
        };
    }

    let dest = state.books_dir.join(format!("{hash}.pdf"));
    if let Err(e) = fs::copy(src, &dest) {
        let _ = fs::remove_file(&dest); // 清理可能的半截文件（如磁盘满）
        return err(format!("复制入库失败：{e}"));
    }

    let title = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "未命名".into());

    let updated_at = now_ms();
    let inserted = db
        .execute(
            "INSERT INTO books (hash, title, file_path, added_at, updated_at)
             VALUES (?1, ?2, ?3, datetime('now', 'localtime'), ?4)",
            rusqlite::params![hash, title, format!("books/{hash}.pdf"), updated_at],
        )
        .and_then(|_| {
            db.query_row(
                &format!("SELECT {BOOK_COLS} FROM books WHERE hash = ?1"),
                [&hash],
                row_to_book,
            )
        });

    match inserted {
        Ok(book) => AddResult {
            path: path_str.to_string(),
            status: "added".into(),
            message: None,
            book: Some(absolutize_book(&state.base_dir, book)),
        },
        Err(e) => {
            let _ = fs::remove_file(&dest);
            err(format!("写入数据库失败：{e}"))
        }
    }
}

fn list_books_in_db(
    db: &Connection,
    base: &Path,
    sort: &str,
    query: &str,
) -> Result<Vec<Book>, String> {
    let order = match sort {
        "added" => "added_at DESC, id DESC",
        "title" => "title COLLATE NOCASE ASC",
        // 默认：最近阅读（没读过的排后面，按添加时间）
        _ => "last_opened_at IS NULL, last_opened_at DESC, added_at DESC",
    };
    let sql = format!(
        "SELECT {BOOK_COLS} FROM books WHERE deleted = 0 AND title LIKE ?1 ORDER BY {order}"
    );
    let mut stmt = db.prepare(&sql).map_err(|e| e.to_string())?;
    let pattern = format!("%{}%", query.trim());
    let books = stmt
        .query_map([pattern], row_to_book)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(books
        .into_iter()
        .map(|b| absolutize_book(base, b))
        .collect())
}

#[tauri::command]
fn list_books(sort: String, query: String, state: State<AppState>) -> Result<Vec<Book>, String> {
    let db = state.db.lock().unwrap();
    list_books_in_db(&db, &state.base_dir, &sort, &query)
}

#[tauri::command]
fn remove_book(id: i64, state: State<AppState>) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    let book = get_book_by_id(&db, id)?;
    tombstone_book(&db, id)?;
    // 删除书库副本与封面缓存；文件删除失败不阻塞（记录已移除）
    let _ = fs::remove_file(to_abs(&state.base_dir, &book.file_path));
    if let Some(cover) = &book.cover_path {
        let _ = fs::remove_file(to_abs(&state.base_dir, cover));
    }
    Ok(())
}

#[tauri::command]
fn update_progress(id: i64, page: i64, state: State<AppState>) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    update_progress_in_db(&db, id, page)
}

#[tauri::command]
fn rename_book(id: i64, title: String, state: State<AppState>) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("书名不能为空".into());
    }
    let db = state.db.lock().unwrap();
    db.execute(
        "UPDATE books SET title = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id, title, now_ms()],
    )
    .map_err(|e| format!("重命名失败：{e}"))?;
    Ok(())
}

#[tauri::command]
fn set_total_pages(id: i64, total: i64, state: State<AppState>) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    db.execute(
        "UPDATE books SET total_pages = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id, total, now_ms()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn save_cover(hash: String, data: Vec<u8>, state: State<AppState>) -> Result<String, String> {
    // hash 来自数据库记录，仍做一次白名单校验避免路径注入
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("非法的 hash".into());
    }
    let path = state.covers_dir.join(format!("{hash}.png"));
    fs::write(&path, data).map_err(|e| format!("保存封面失败：{e}"))?;
    let db = state.db.lock().unwrap();
    db.execute(
        "UPDATE books SET cover_path = ?2, updated_at = ?3 WHERE hash = ?1",
        rusqlite::params![hash, format!("covers/{hash}.png"), now_ms()],
    )
    .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[derive(Serialize)]
struct LookupResult {
    found: bool,
    word: String,
    lemma: Option<String>,
    phonetic: Option<String>,
    translation: Option<String>,
}

/// 简单后缀剥离规则：forms 表未命中时的兜底（-s/-es/-ies/-ing/-ed/-er/-est 等）
fn suffix_candidates(w: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if s.len() >= 2 && !out.contains(&s) {
            out.push(s);
        }
    };
    let dedup_double = |b: &str| -> Option<String> {
        let bytes = b.as_bytes();
        if bytes.len() >= 2 && bytes[bytes.len() - 1] == bytes[bytes.len() - 2] {
            Some(b[..b.len() - 1].to_string())
        } else {
            None
        }
    };
    if let Some(b) = w.strip_suffix("ies") {
        push(format!("{b}y"));
    }
    if let Some(b) = w.strip_suffix("es") {
        push(b.to_string());
    }
    if let Some(b) = w.strip_suffix('s') {
        push(b.to_string());
    }
    for suf in ["ing", "ed"] {
        if let Some(b) = w.strip_suffix(suf) {
            push(b.to_string());
            push(format!("{b}e"));
            if let Some(d) = dedup_double(b) {
                push(d);
            }
        }
    }
    if let Some(b) = w.strip_suffix("iest") {
        push(format!("{b}y"));
    }
    if let Some(b) = w.strip_suffix("ier") {
        push(format!("{b}y"));
    }
    for suf in ["est", "er"] {
        if let Some(b) = w.strip_suffix(suf) {
            push(b.to_string());
            push(format!("{b}e"));
            if let Some(d) = dedup_double(b) {
                push(d);
            }
        }
    }
    out
}

#[tauri::command]
fn lookup_word(word: String, state: State<AppState>) -> LookupResult {
    let guard = state.dict.lock().unwrap();
    do_lookup(guard.as_ref(), &word)
}

fn do_lookup(dict: Option<&Connection>, word: &str) -> LookupResult {
    let original = word.trim().to_string();
    let miss = |w: String| LookupResult {
        found: false,
        word: w,
        lemma: None,
        phonetic: None,
        translation: None,
    };
    if original.is_empty() || original.len() > 50 {
        return miss(original);
    }
    let w = original.to_lowercase();
    if !w
        .chars()
        .all(|c| c.is_ascii_lowercase() || c == '\'' || c == '-')
    {
        return miss(original);
    }

    let Some(db) = dict else {
        return miss(original);
    };

    let get = |target: &str| -> Option<(Option<String>, String)> {
        db.query_row(
            "SELECT phonetic, translation FROM entries WHERE word = ?1",
            [target],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
    };

    // ① 精确命中
    if let Some((phonetic, translation)) = get(&w) {
        return LookupResult {
            found: true,
            word: original,
            lemma: None,
            phonetic,
            translation: Some(translation),
        };
    }
    // ② forms 反查词形还原
    if let Ok(lemma) = db.query_row("SELECT lemma FROM forms WHERE form = ?1", [&w], |r| {
        r.get::<_, String>(0)
    }) {
        if let Some((phonetic, translation)) = get(&lemma) {
            return LookupResult {
                found: true,
                word: original,
                lemma: Some(lemma),
                phonetic,
                translation: Some(translation),
            };
        }
    }
    // ③ 后缀规则兜底
    for cand in suffix_candidates(&w) {
        if let Some((phonetic, translation)) = get(&cand) {
            return LookupResult {
                found: true,
                word: original,
                lemma: Some(cand),
                phonetic,
                translation: Some(translation),
            };
        }
    }
    miss(original)
}

fn open_dict(app: &tauri::App, base: &Path) -> Option<Connection> {
    use tauri::path::BaseDirectory;
    // 用户目录的 dict.db 优先（可手动替换升级），否则用安装目录内置资源
    let user_dict = base.join("dict.db");
    let path = if user_dict.is_file() {
        user_dict
    } else {
        app.path()
            .resolve("resources/dict.db", BaseDirectory::Resource)
            .ok()
            .filter(|p| p.is_file())?
    };
    Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
}

fn init_db(base: &Path) -> Result<Connection, Box<dyn std::error::Error>> {
    let db = Connection::open(base.join("library.db"))?;
    // WAL：保证强杀进程时已提交的进度不丢
    db.pragma_update(None, "journal_mode", "WAL")?;
    init_library_db(&db)?;
    Ok(db)
}

fn init_library_db(db: &Connection) -> rusqlite::Result<()> {
    create_schema(db)?;
    migrate_schema(db)?;
    normalize_book_paths(db)?;
    Ok(())
}

fn create_schema(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS books (
            id INTEGER PRIMARY KEY,
            hash TEXT UNIQUE NOT NULL,
            title TEXT NOT NULL,
            file_path TEXT NOT NULL,
            cover_path TEXT,
            total_pages INTEGER,
            current_page INTEGER NOT NULL DEFAULT 1,
            added_at TEXT NOT NULL,
            last_opened_at TEXT
        );",
    )
}

fn migrate_schema(db: &Connection) -> rusqlite::Result<()> {
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version >= 2 {
        return Ok(());
    }

    db.execute_batch(
        "BEGIN IMMEDIATE;
         ALTER TABLE books ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE books ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE books ADD COLUMN synced_at INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE books ADD COLUMN cloud_state TEXT NOT NULL DEFAULT 'local';
         CREATE TABLE IF NOT EXISTS sync_meta (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );
         PRAGMA user_version = 2;
         COMMIT;",
    )?;
    Ok(())
}

/// 旧版本（≤v0.2.2）把绝对路径写入 file_path/cover_path，数据目录一迁移就全部悬空。
/// 这里把存量绝对路径按文件名改写为相对路径（幂等，已是相对路径的行不动）。
/// 入库文件从来都是按 "<base>/books/<hash>.pdf" 布局落盘，所以取文件名重挂是安全的。
fn normalize_book_paths(db: &Connection) -> rusqlite::Result<()> {
    let rows: Vec<(i64, String, Option<String>)> = db
        .prepare("SELECT id, file_path, cover_path FROM books")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    for (id, file_path, cover_path) in rows {
        let new_file = to_relative(&file_path, "books");
        let new_cover = cover_path.as_deref().and_then(|c| to_relative(c, "covers"));
        if new_file.is_some() || new_cover.is_some() {
            db.execute(
                "UPDATE books SET file_path = COALESCE(?2, file_path),
                                  cover_path = COALESCE(?3, cover_path) WHERE id = ?1",
                rusqlite::params![id, new_file, new_cover],
            )?;
        }
    }
    Ok(())
}

/// 绝对路径 → "subdir/文件名"；已是相对路径返回 None（表示无需改写）
fn to_relative(stored: &str, subdir: &str) -> Option<String> {
    let p = Path::new(stored);
    if !p.is_absolute() {
        return None;
    }
    let name = p.file_name()?.to_string_lossy();
    Some(format!("{subdir}/{name}"))
}

#[cfg(windows)]
fn resolve_base_dir(app: &tauri::App) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Tauri 2 会把应用数据目录解析到 %APPDATA%\com.shelf.reader。
    // 旧版本硬编码使用 %APPDATA%\Shelf，这里做一次性迁移，避免老用户升级后读不到原数据。
    let new_base = app.path().app_data_dir()?;
    let old_base = PathBuf::from(std::env::var("APPDATA")?).join("Shelf");
    Ok(migrate_old_base(&old_base, &new_base))
}

#[cfg(not(windows))]
fn resolve_base_dir(app: &tauri::App) -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(app.path().app_data_dir()?)
}

/// 决定实际使用的数据目录，需要时把旧目录整体迁移到新目录。
/// 判据是"哪边有 library.db"而不是"目录是否存在"——启动中途失败等原因
/// 可能残留一个空目录骨架，不能因此永久跳过迁移让用户看到空书库。
/// 所有异常路径都回退到能读到数据的那个目录，绝不做有数据风险的操作。
#[cfg_attr(not(windows), allow(dead_code))]
fn migrate_old_base(old_base: &Path, new_base: &Path) -> PathBuf {
    if !old_base.join("library.db").is_file() || new_base.join("library.db").is_file() {
        // 没有旧数据可迁，或新目录已有书库（已迁移过/全新数据）：直接用新目录
        return new_base.to_path_buf();
    }
    // 新目录若已有实际文件（异常状态，无法安全合并），继续用旧目录；
    // 只是空骨架则清掉，让整体重命名可以原子完成
    if new_base.exists() && (!is_empty_skeleton(new_base) || fs::remove_dir_all(new_base).is_err())
    {
        return old_base.to_path_buf();
    }
    // 同卷原子重命名；失败（如文件被占用）则继续用旧目录，下次启动再试
    if fs::rename(old_base, new_base).is_err() {
        return old_base.to_path_buf();
    }
    new_base.to_path_buf()
}

/// 目录树中不含任何文件（只有空目录，或目录本身为空）
#[cfg_attr(not(windows), allow(dead_code))]
fn is_empty_skeleton(dir: &Path) -> bool {
    match fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .all(|e| e.path().is_dir() && is_empty_skeleton(&e.path())),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    fn dict() -> Connection {
        Connection::open_with_flags(
            "resources/dict.db",
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("测试需要 resources/dict.db 存在")
    }

    fn memory_library_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        init_library_db(&db).unwrap();
        db
    }

    fn insert_test_book(db: &Connection, hash: &str, title: &str) -> i64 {
        db.execute(
            "INSERT INTO books (hash, title, file_path, added_at, updated_at)
             VALUES (?1, ?2, ?3, '2026-01-01', 1)",
            rusqlite::params![hash, title, format!("books/{hash}.pdf")],
        )
        .unwrap();
        db.last_insert_rowid()
    }

    #[test]
    fn exact_hit() {
        let db = dict();
        let r = do_lookup(Some(&db), "hello");
        assert!(r.found);
        assert!(r.lemma.is_none());
        assert!(r.translation.unwrap().contains("喂"));
    }

    #[test]
    fn case_insensitive_and_lemma() {
        let db = dict();
        // 句首大写
        let r = do_lookup(Some(&db), "The");
        assert!(r.found);
        // 变形词：要么本身是词条（精确命中，lemma 为空），要么正确还原到原形
        for (form, lemma) in [
            ("running", "run"),
            ("studies", "study"),
            ("went", "go"),
            ("better", "good"),
            ("children", "child"),
            ("abandons", "abandon"),
        ] {
            let r = do_lookup(Some(&db), form);
            assert!(r.found, "{form} 应命中");
            if let Some(l) = r.lemma.as_deref() {
                assert_eq!(l, lemma, "{form} 应还原为 {lemma}");
            }
        }
    }

    #[test]
    fn invalid_input_rejected() {
        let db = dict();
        for bad in ["", "你好", "123", "  ", "hello world", "a@b"] {
            assert!(!do_lookup(Some(&db), bad).found, "{bad:?} 不应命中");
        }
        let long = "a".repeat(51);
        assert!(!do_lookup(Some(&db), &long).found);
    }

    #[test]
    fn not_found_word() {
        let db = dict();
        let r = do_lookup(Some(&db), "zzzzqqq");
        assert!(!r.found);
        assert_eq!(r.word, "zzzzqqq");
    }

    #[test]
    fn missing_dict_degrades() {
        assert!(!do_lookup(None, "hello").found);
    }

    #[test]
    fn suffix_rules() {
        assert!(suffix_candidates("boxes").contains(&"box".to_string()));
        assert!(suffix_candidates("running").contains(&"run".to_string()));
        assert!(suffix_candidates("hoping").contains(&"hope".to_string()));
        assert!(suffix_candidates("happiest").contains(&"happy".to_string()));
    }

    // ---- SQLite v2 迁移 / 墓碑 / updated_at ----

    #[test]
    fn migrate_schema_sets_user_version_2_and_is_idempotent() {
        let db = Connection::open_in_memory().unwrap();
        create_schema(&db).unwrap();

        migrate_schema(&db).unwrap();
        let version: i64 = db
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);

        let sync_meta_exists: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'sync_meta'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sync_meta_exists, 1);

        migrate_schema(&db).unwrap();
        let version_again: i64 = db
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version_again, 2);
    }

    #[test]
    fn tombstoned_book_is_hidden_from_list_books() {
        let db = memory_library_db();
        let id = insert_test_book(&db, "deadbeef", "墓碑书");

        tombstone_book(&db, id).unwrap();

        let base = std::env::temp_dir();
        let books = list_books_in_db(&db, &base, "added", "").unwrap();
        assert!(books.is_empty());
        let deleted: i64 = db
            .query_row("SELECT deleted FROM books WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(deleted, 1);
    }

    #[test]
    fn readd_same_hash_revives_deleted_book() {
        let db = memory_library_db();
        let id = insert_test_book(&db, "cafebabe", "已删除旧书");
        db.execute("UPDATE books SET deleted = 1 WHERE id = ?1", [id])
            .unwrap();

        revive_deleted_book_record(&db, id, "books/cafebabe.pdf").unwrap();

        let (deleted, file_path): (i64, String) = db
            .query_row(
                "SELECT deleted, file_path FROM books WHERE hash = 'cafebabe'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(file_path, "books/cafebabe.pdf");
    }

    #[test]
    fn update_progress_advances_updated_at() {
        let db = memory_library_db();
        let id = insert_test_book(&db, "feedface", "进度书");
        let before: i64 = db
            .query_row("SELECT updated_at FROM books WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .unwrap();

        thread::sleep(Duration::from_millis(2));
        update_progress_in_db(&db, id, 42).unwrap();

        let (current_page, after): (i64, i64) = db
            .query_row(
                "SELECT current_page, updated_at FROM books WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(current_page, 42);
        assert!(after > before, "updated_at 应前进");
    }

    // ---- 路径相对化（老库绝对路径改写）----

    #[test]
    fn normalize_rewrites_absolute_paths() {
        let db = Connection::open_in_memory().unwrap();
        create_schema(&db).unwrap();
        // 用 temp_dir 构造平台各自格式的绝对路径（Windows 盘符 / Unix 斜杠都覆盖）
        let abs_book = std::env::temp_dir().join("books").join("abc.pdf");
        let abs_cover = std::env::temp_dir().join("covers").join("abc.png");
        db.execute(
            "INSERT INTO books (hash, title, file_path, cover_path, added_at)
             VALUES ('abc', 't', ?1, ?2, '2026-01-01')",
            rusqlite::params![abs_book.to_string_lossy(), abs_cover.to_string_lossy()],
        )
        .unwrap();
        normalize_book_paths(&db).unwrap();
        let (f, c): (String, String) = db
            .query_row("SELECT file_path, cover_path FROM books", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(f, "books/abc.pdf");
        assert_eq!(c, "covers/abc.png");
    }

    #[test]
    fn normalize_is_idempotent_and_keeps_null_cover() {
        let db = Connection::open_in_memory().unwrap();
        create_schema(&db).unwrap();
        db.execute(
            "INSERT INTO books (hash, title, file_path, added_at)
             VALUES ('abc', 't', 'books/abc.pdf', '2026-01-01')",
            [],
        )
        .unwrap();
        normalize_book_paths(&db).unwrap();
        let (f, c): (String, Option<String>) = db
            .query_row("SELECT file_path, cover_path FROM books", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(f, "books/abc.pdf");
        assert!(c.is_none());
    }

    #[test]
    fn to_abs_joins_relative_segments() {
        let base = std::env::temp_dir();
        let expect = base
            .join("books")
            .join("x.pdf")
            .to_string_lossy()
            .to_string();
        assert_eq!(to_abs(&base, "books/x.pdf"), expect);
    }

    // ---- 数据目录迁移 ----

    fn fresh_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("shelf-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_old_library(old: &Path) {
        fs::create_dir_all(old.join("books")).unwrap();
        fs::write(old.join("library.db"), b"db").unwrap();
        fs::write(old.join("books").join("a.pdf"), b"x").unwrap();
    }

    #[test]
    fn migrate_moves_old_dir_atomically() {
        let root = fresh_dir("mig-move");
        let (old, new) = (root.join("old"), root.join("new"));
        make_old_library(&old);
        assert_eq!(migrate_old_base(&old, &new), new);
        assert!(new.join("library.db").is_file());
        assert!(new.join("books").join("a.pdf").is_file());
        assert!(!old.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn migrate_clears_empty_skeleton_then_moves() {
        let root = fresh_dir("mig-skeleton");
        let (old, new) = (root.join("old"), root.join("new"));
        make_old_library(&old);
        // 上次启动中途失败残留的空骨架不能阻断迁移
        fs::create_dir_all(new.join("books")).unwrap();
        fs::create_dir_all(new.join("covers")).unwrap();
        assert_eq!(migrate_old_base(&old, &new), new);
        assert!(new.join("library.db").is_file());
        assert!(!old.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn migrate_skips_when_new_already_has_db() {
        let root = fresh_dir("mig-hasdb");
        let (old, new) = (root.join("old"), root.join("new"));
        make_old_library(&old);
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("library.db"), b"newer").unwrap();
        assert_eq!(migrate_old_base(&old, &new), new);
        // 两边数据都原样保留，旧目录留作备份
        assert!(old.join("library.db").is_file());
        assert_eq!(fs::read(new.join("library.db")).unwrap(), b"newer");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn migrate_keeps_old_when_new_has_files_but_no_db() {
        let root = fresh_dir("mig-dirty");
        let (old, new) = (root.join("old"), root.join("new"));
        make_old_library(&old);
        fs::create_dir_all(new.join("books")).unwrap();
        fs::write(new.join("books").join("stray.pdf"), b"y").unwrap();
        // 新目录有真实文件但没有书库：无法安全合并，继续用旧目录
        assert_eq!(migrate_old_base(&old, &new), old);
        assert!(old.join("library.db").is_file());
        assert!(new.join("books").join("stray.pdf").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn migrate_fresh_install_uses_new() {
        let root = fresh_dir("mig-fresh");
        let (old, new) = (root.join("old"), root.join("new"));
        // 老目录不存在（全新安装）
        assert_eq!(migrate_old_base(&old, &new), new);
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let base = resolve_base_dir(app)?;
            let books_dir = base.join("books");
            let covers_dir = base.join("covers");
            fs::create_dir_all(&books_dir)?;
            fs::create_dir_all(&covers_dir)?;
            // 运行时把书库目录加入 asset 协议白名单；
            // 配置文件里的 glob 作用域在 Windows 反斜杠路径下匹配不可靠（会 403）
            let scope = app.asset_protocol_scope();
            scope.allow_directory(&books_dir, true)?;
            scope.allow_directory(&covers_dir, true)?;
            let db = init_db(&base)?;
            let dict = open_dict(app, &base);
            app.manage(AppState {
                db: Mutex::new(db),
                dict: Mutex::new(dict),
                base_dir: base,
                books_dir,
                covers_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            add_books,
            list_books,
            remove_book,
            update_progress,
            rename_book,
            set_total_pages,
            save_cover,
            lookup_word
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
