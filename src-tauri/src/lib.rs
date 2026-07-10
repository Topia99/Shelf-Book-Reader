use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{Manager, State};

struct AppState {
    db: Mutex<Connection>,
    /// 只读词典库；缺失时为 None，查词一律返回未找到
    dict: Mutex<Option<Connection>>,
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

fn sha256_of_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("无法打开文件：{e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("读取文件失败：{e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn get_book_by_id(db: &Connection, id: i64) -> Result<Book, String> {
    db.query_row(
        &format!("SELECT {BOOK_COLS} FROM books WHERE id = ?1"),
        [id],
        row_to_book,
    )
    .map_err(|e| format!("找不到书籍：{e}"))
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

    // 查重：同哈希即同一本书（即使文件名不同）
    if let Ok(existing) = db.query_row(
        &format!("SELECT {BOOK_COLS} FROM books WHERE hash = ?1"),
        [&hash],
        row_to_book,
    ) {
        return AddResult {
            path: path_str.to_string(),
            status: "duplicate".into(),
            message: Some(format!("已在书库中：{}", existing.title)),
            book: Some(existing),
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

    let inserted = db
        .execute(
            "INSERT INTO books (hash, title, file_path, added_at)
             VALUES (?1, ?2, ?3, datetime('now', 'localtime'))",
            rusqlite::params![hash, title, dest.to_string_lossy()],
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
            book: Some(book),
        },
        Err(e) => {
            let _ = fs::remove_file(&dest);
            err(format!("写入数据库失败：{e}"))
        }
    }
}

#[tauri::command]
fn list_books(sort: String, query: String, state: State<AppState>) -> Result<Vec<Book>, String> {
    let order = match sort.as_str() {
        "added" => "added_at DESC, id DESC",
        "title" => "title COLLATE NOCASE ASC",
        // 默认：最近阅读（没读过的排后面，按添加时间）
        _ => "last_opened_at IS NULL, last_opened_at DESC, added_at DESC",
    };
    let db = state.db.lock().unwrap();
    let sql = format!("SELECT {BOOK_COLS} FROM books WHERE title LIKE ?1 ORDER BY {order}");
    let mut stmt = db.prepare(&sql).map_err(|e| e.to_string())?;
    let pattern = format!("%{}%", query.trim());
    let books = stmt
        .query_map([pattern], row_to_book)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(books)
}

#[tauri::command]
fn remove_book(id: i64, state: State<AppState>) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    let book = get_book_by_id(&db, id)?;
    db.execute("DELETE FROM books WHERE id = ?1", [id])
        .map_err(|e| format!("删除记录失败：{e}"))?;
    // 删除书库副本与封面缓存；文件删除失败不阻塞（记录已移除）
    let _ = fs::remove_file(&book.file_path);
    if let Some(cover) = &book.cover_path {
        let _ = fs::remove_file(cover);
    }
    Ok(())
}

#[tauri::command]
fn update_progress(id: i64, page: i64, state: State<AppState>) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    db.execute(
        "UPDATE books SET current_page = ?2, last_opened_at = datetime('now', 'localtime') WHERE id = ?1",
        rusqlite::params![id, page],
    )
    .map_err(|e| format!("保存进度失败：{e}"))?;
    Ok(())
}

#[tauri::command]
fn rename_book(id: i64, title: String, state: State<AppState>) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("书名不能为空".into());
    }
    let db = state.db.lock().unwrap();
    db.execute(
        "UPDATE books SET title = ?2 WHERE id = ?1",
        rusqlite::params![id, title],
    )
    .map_err(|e| format!("重命名失败：{e}"))?;
    Ok(())
}

#[tauri::command]
fn set_total_pages(id: i64, total: i64, state: State<AppState>) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    db.execute(
        "UPDATE books SET total_pages = ?2 WHERE id = ?1",
        rusqlite::params![id, total],
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
    let path_str = path.to_string_lossy().to_string();
    let db = state.db.lock().unwrap();
    db.execute(
        "UPDATE books SET cover_path = ?2 WHERE hash = ?1",
        rusqlite::params![hash, path_str],
    )
    .map_err(|e| e.to_string())?;
    Ok(path_str)
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
    if let Ok(lemma) =
        db.query_row("SELECT lemma FROM forms WHERE form = ?1", [&w], |r| {
            r.get::<_, String>(0)
        })
    {
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
    )?;
    Ok(db)
}

#[cfg(windows)]
fn resolve_base_dir(app: &tauri::App) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Tauri 2 会把应用数据目录解析到 %APPDATA%\com.shelf.reader。
    // 旧版本硬编码使用 %APPDATA%\Shelf，这里做一次性迁移，避免老用户升级后读不到原数据。
    let new_base = app.path().app_data_dir()?;
    let old_base = PathBuf::from(std::env::var("APPDATA")?).join("Shelf");
    if old_base.exists() && !new_base.exists() {
        if fs::rename(&old_base, &new_base).is_err() {
            return Ok(old_base);
        }
    }
    Ok(new_base)
}

#[cfg(not(windows))]
fn resolve_base_dir(app: &tauri::App) -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(app.path().app_data_dir()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict() -> Connection {
        Connection::open_with_flags(
            "resources/dict.db",
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("测试需要 resources/dict.db 存在")
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
