use crate::sync::{CloudBook, CloudProgress};
use rusqlite::{params, Connection, OptionalExtension};

const PROGRESS_MERGE_WINDOW_MS: i64 = 120_000;

#[derive(Debug, PartialEq, Default)]
pub(crate) struct MergeStats {
    pub(crate) inserted: usize,
    pub(crate) updated: usize,
    pub(crate) skipped: usize,
}

pub(crate) fn collect_dirty(
    db: &Connection,
) -> rusqlite::Result<(Vec<CloudBook>, Vec<CloudProgress>)> {
    let mut stmt = db.prepare(
        "SELECT hash, title, current_page, updated_at, deleted
         FROM books
         WHERE updated_at > synced_at
         ORDER BY id",
    )?;
    let rows: Vec<(String, String, i64, i64, i64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<Result<_, _>>()?;

    let books = rows
        .iter()
        .map(|(hash, title, _page, updated_at, deleted)| CloudBook {
            sha256: hash.clone(),
            title: title.clone(),
            author: None,
            page_count: None,
            // 本地暂时没有文件大小列，阶段 5 再接真实值。
            file_size: 0,
            cover_key: None,
            file_key: None,
            updated_at: *updated_at,
            deleted: *deleted != 0,
        })
        .collect();
    let progress = rows
        .into_iter()
        .map(|(hash, _title, page, updated_at, _deleted)| CloudProgress {
            sha256: hash,
            page,
            zoom_mode: None,
            view_mode: None,
            device_name: None,
            updated_at,
        })
        .collect();

    Ok((books, progress))
}

pub(crate) fn mark_synced(
    db: &Connection,
    hashes: &[String],
    synced_at_ms: i64,
) -> rusqlite::Result<()> {
    let mut stmt = db.prepare("UPDATE books SET synced_at = ?2 WHERE hash = ?1")?;
    for hash in hashes {
        stmt.execute(params![hash, synced_at_ms])?;
    }
    Ok(())
}

/// 待上传文件的候选：本机有文件、尚未上传到云（cloud_state='local'、未删除）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UploadCandidate {
    pub hash: String,
    /// 相对 base 的文件路径（如 books/<hash>.pdf），读取时由调用方拼绝对。
    pub file_path: String,
}

/// 收集需要上传文件本体的本地书。元数据 push 之后调用，云端行已存在可回填 file_key。
pub(crate) fn collect_uploadable(db: &Connection) -> rusqlite::Result<Vec<UploadCandidate>> {
    let mut stmt = db.prepare(
        "SELECT hash, file_path FROM books
         WHERE cloud_state = 'local' AND deleted = 0
         ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(UploadCandidate {
                hash: r.get(0)?,
                file_path: r.get(1)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// 更新单本书的 cloud_state（local | uploading | synced | remote）。
pub(crate) fn set_cloud_state(db: &Connection, hash: &str, state: &str) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE books SET cloud_state = ?2 WHERE hash = ?1",
        params![hash, state],
    )?;
    Ok(())
}

pub(crate) fn merge_remote_books(
    db: &Connection,
    rows: &[CloudBook],
    now_ms: i64,
) -> rusqlite::Result<MergeStats> {
    let mut stats = MergeStats::default();
    let mut select_stmt = db.prepare(
        "SELECT id, title, current_page, updated_at, synced_at, deleted
         FROM books
         WHERE hash = ?1",
    )?;
    let mut insert_stmt = db.prepare(
        "INSERT INTO books (
             hash, title, file_path, current_page, added_at, last_opened_at,
             updated_at, deleted, synced_at, cloud_state
         ) VALUES (
             ?1, ?2, '', 1, datetime(?3 / 1000, 'unixepoch', 'localtime'), NULL,
             ?4, ?5, ?4, 'remote'
         )",
    )?;
    let mut update_stmt = db.prepare(
        "UPDATE books
         SET title = ?2,
             deleted = ?3,
             updated_at = ?4,
             synced_at = ?4,
             cloud_state = 'remote'
         WHERE id = ?1",
    )?;

    for remote in rows {
        let local = select_stmt
            .query_row([&remote.sha256], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .optional()?;

        match local {
            None => {
                insert_stmt.execute(params![
                    remote.sha256,
                    remote.title,
                    now_ms,
                    remote.updated_at,
                    if remote.deleted { 1 } else { 0 }
                ])?;
                stats.inserted += 1;
            }
            Some((
                id,
                local_title,
                _local_page,
                local_updated_at,
                local_synced_at,
                local_deleted,
            )) => {
                if remote.updated_at <= local_updated_at {
                    stats.skipped += 1;
                    continue;
                }

                let local_is_dirty = local_updated_at > local_synced_at;
                let local_deleted_bool = local_deleted != 0;
                if !local_is_dirty
                    && local_title == remote.title
                    && local_deleted_bool == remote.deleted
                    && local_synced_at >= remote.updated_at
                {
                    stats.skipped += 1;
                    continue;
                }

                update_stmt.execute(params![
                    id,
                    remote.title,
                    if remote.deleted { 1 } else { 0 },
                    remote.updated_at
                ])?;
                stats.updated += 1;
            }
        }
    }

    Ok(stats)
}

pub(crate) fn merge_remote_progress(
    db: &Connection,
    rows: &[CloudProgress],
) -> rusqlite::Result<MergeStats> {
    let mut stats = MergeStats::default();
    let mut select_stmt = db.prepare(
        "SELECT id, current_page, updated_at, synced_at
         FROM books
         WHERE hash = ?1",
    )?;
    let mut update_stmt = db.prepare(
        "UPDATE books
         SET current_page = ?2,
             updated_at = ?3,
             synced_at = ?4
         WHERE id = ?1",
    )?;

    for remote in rows {
        let local = select_stmt
            .query_row([&remote.sha256], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .optional()?;

        let Some((id, local_page, local_updated_at, local_synced_at)) = local else {
            // 书籍元数据尚未落到本地时先忽略进度，避免造出孤儿记录。
            stats.skipped += 1;
            continue;
        };

        let within_window = (remote.updated_at - local_updated_at).abs() < PROGRESS_MERGE_WINDOW_MS;
        let (target_page, target_updated_at) = if within_window {
            (
                local_page.max(remote.page),
                local_updated_at.max(remote.updated_at),
            )
        } else if remote.updated_at > local_updated_at {
            (remote.page, remote.updated_at)
        } else {
            (local_page, local_updated_at)
        };
        // synced_at 语义：只有"采纳了远端值"才允许推进（行变干净）。
        // 本地值胜出时云端还没有这个值，必须保持 updated_at > synced_at 的脏状态，
        // 让下一轮 push 把本地值送上去（幂等 upsert，收敛后 no-change 跳过，不会循环）。
        let adopted_remote = target_page == remote.page;
        let (target_updated_at, target_synced_at) = if adopted_remote {
            (
                target_updated_at,
                local_synced_at.max(target_updated_at).max(remote.updated_at),
            )
        } else {
            // 罕见时钟偏差下本地行可能本是干净的，抬高 updated_at 保证脏状态成立
            (target_updated_at.max(local_synced_at + 1), local_synced_at)
        };

        if target_page == local_page
            && target_updated_at == local_updated_at
            && target_synced_at == local_synced_at
        {
            stats.skipped += 1;
            continue;
        }

        update_stmt.execute(params![
            id,
            target_page,
            target_updated_at,
            target_synced_at
        ])?;
        stats.updated += 1;
    }

    Ok(stats)
}

pub(crate) fn get_cursor(db: &Connection) -> rusqlite::Result<Option<String>> {
    db.query_row(
        "SELECT value FROM sync_meta WHERE key = 'pull_cursor'",
        [],
        |row| row.get(0),
    )
    .optional()
}

pub(crate) fn set_cursor(db: &Connection, cursor: &str) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO sync_meta (key, value)
         VALUES ('pull_cursor', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [cursor],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_library_db;

    fn memory_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        init_library_db(&db).unwrap();
        db
    }

    fn insert_book_row(
        db: &Connection,
        hash: &str,
        title: &str,
        current_page: i64,
        updated_at: i64,
        synced_at: i64,
        deleted: i64,
    ) {
        db.execute(
            "INSERT INTO books (
                 hash, title, file_path, current_page, added_at,
                 updated_at, synced_at, deleted, cloud_state
             ) VALUES (
                 ?1, ?2, ?3, ?4, '2026-01-01', ?5, ?6, ?7, 'local'
             )",
            params![
                hash,
                title,
                format!("books/{hash}.pdf"),
                current_page,
                updated_at,
                synced_at,
                deleted
            ],
        )
        .unwrap();
    }

    fn cloud_book(hash: &str, title: &str, updated_at: i64, deleted: bool) -> CloudBook {
        CloudBook {
            sha256: hash.to_string(),
            title: title.to_string(),
            author: None,
            page_count: None,
            file_size: 0,
            cover_key: None,
            file_key: None,
            updated_at,
            deleted,
        }
    }

    fn cloud_progress(hash: &str, page: i64, updated_at: i64) -> CloudProgress {
        CloudProgress {
            sha256: hash.to_string(),
            page,
            zoom_mode: None,
            view_mode: None,
            device_name: None,
            updated_at,
        }
    }

    #[test]
    fn collect_uploadable_只返回_local_未删除的书() {
        let db = memory_db();
        insert_book_row(&db, "h1", "书一", 1, 100, 0, 0); // local，待上传
        insert_book_row(&db, "h2", "书二", 1, 100, 0, 1); // local 但已删除，跳过
        insert_book_row(&db, "h3", "书三", 1, 100, 0, 0);
        set_cloud_state(&db, "h3", "synced").unwrap(); // 已同步，跳过
        insert_book_row(&db, "h4", "书四", 1, 100, 0, 0);
        set_cloud_state(&db, "h4", "remote").unwrap(); // 云端书，跳过

        let items = collect_uploadable(&db).unwrap();
        let hashes: Vec<&str> = items.iter().map(|c| c.hash.as_str()).collect();
        assert_eq!(hashes, vec!["h1"]);
        assert_eq!(items[0].file_path, "books/h1.pdf");
    }

    #[test]
    fn set_cloud_state_更新指定书的状态() {
        let db = memory_db();
        insert_book_row(&db, "h1", "书一", 1, 100, 0, 0);
        set_cloud_state(&db, "h1", "uploading").unwrap();
        let state: String = db
            .query_row("SELECT cloud_state FROM books WHERE hash='h1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(state, "uploading");
    }

    #[test]
    fn collect_dirty_干净库返回空集合() {
        let db = memory_db();

        let (books, progress) = collect_dirty(&db).unwrap();

        assert!(books.is_empty());
        assert!(progress.is_empty());
    }

    #[test]
    fn collect_dirty_进度变更后同时产出书籍与进度行() {
        let db = memory_db();
        insert_book_row(&db, "h1", "书一", 10, 200, 100, 0);

        let (books, progress) = collect_dirty(&db).unwrap();

        assert_eq!(books.len(), 1);
        assert_eq!(books[0], cloud_book("h1", "书一", 200, false));
        assert_eq!(progress.len(), 1);
        assert_eq!(progress[0], cloud_progress("h1", 10, 200));
    }

    #[test]
    fn collect_dirty_mark_synced_后脏行消失() {
        let db = memory_db();
        insert_book_row(&db, "h1", "书一", 10, 200, 100, 0);

        mark_synced(&db, &[String::from("h1")], 200).unwrap();
        let (books, progress) = collect_dirty(&db).unwrap();

        assert!(books.is_empty());
        assert!(progress.is_empty());
    }

    #[test]
    fn merge_books_远端新书插入为_remote() {
        let db = memory_db();

        let stats =
            merge_remote_books(&db, &[cloud_book("h1", "远端书", 300, false)], 9_999).unwrap();

        assert_eq!(
            stats,
            MergeStats {
                inserted: 1,
                updated: 0,
                skipped: 0
            }
        );
        let row: (String, String, i64, i64, i64) = db
            .query_row(
                "SELECT file_path, cloud_state, updated_at, synced_at, deleted FROM books WHERE hash = 'h1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(row.0, "");
        assert_eq!(row.1, "remote");
        assert_eq!(row.2, 300);
        assert_eq!(row.3, 300);
        assert_eq!(row.4, 0);
    }

    #[test]
    fn merge_books_远端旧数据不覆盖本地新数据() {
        let db = memory_db();
        insert_book_row(&db, "h1", "本地新", 8, 400, 400, 0);

        let stats = merge_remote_books(&db, &[cloud_book("h1", "远端旧", 300, false)], 0).unwrap();

        assert_eq!(stats.skipped, 1);
        let title: String = db
            .query_row("SELECT title FROM books WHERE hash = 'h1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(title, "本地新");
    }

    #[test]
    fn merge_books_远端新数据覆盖本地旧数据() {
        let db = memory_db();
        insert_book_row(&db, "h1", "本地旧", 8, 200, 200, 0);

        let stats = merge_remote_books(&db, &[cloud_book("h1", "远端新", 300, false)], 0).unwrap();

        assert_eq!(stats.updated, 1);
        let row: (String, i64, i64, String) = db
            .query_row(
                "SELECT title, updated_at, synced_at, cloud_state FROM books WHERE hash = 'h1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(row.0, "远端新");
        assert_eq!(row.1, 300);
        assert_eq!(row.2, 300);
        assert_eq!(row.3, "remote");
    }

    #[test]
    fn merge_books_远端墓碑会传播到本地() {
        let db = memory_db();
        insert_book_row(&db, "h1", "书一", 8, 200, 200, 0);

        let stats = merge_remote_books(&db, &[cloud_book("h1", "书一", 300, true)], 0).unwrap();

        assert_eq!(stats.updated, 1);
        let deleted: i64 = db
            .query_row("SELECT deleted FROM books WHERE hash = 'h1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(deleted, 1);
    }

    #[test]
    fn merge_books_本地脏且远端更新时仍按_lww_采用远端元数据() {
        let db = memory_db();
        insert_book_row(&db, "h1", "本地脏", 55, 250, 100, 0);

        let stats = merge_remote_books(&db, &[cloud_book("h1", "远端胜", 300, false)], 0).unwrap();

        assert_eq!(stats.updated, 1);
        let row: (String, i64, i64, i64) = db
            .query_row(
                "SELECT title, current_page, updated_at, synced_at FROM books WHERE hash = 'h1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(row.0, "远端胜");
        assert_eq!(row.1, 55);
        assert_eq!(row.2, 300);
        assert_eq!(row.3, 300);
    }

    #[test]
    fn merge_progress_两分钟窗口内远端页码更大则采用远端页码() {
        let db = memory_db();
        insert_book_row(&db, "h1", "书一", 10, 1_000, 900, 0);

        let stats = merge_remote_progress(&db, &[cloud_progress("h1", 20, 1_050)]).unwrap();

        assert_eq!(stats.updated, 1);
        let row: (i64, i64, i64) = db
            .query_row(
                "SELECT current_page, updated_at, synced_at FROM books WHERE hash = 'h1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, (20, 1_050, 1_050));
    }

    #[test]
    fn merge_progress_两分钟窗口内本地页码更大则保留本地页码() {
        let db = memory_db();
        insert_book_row(&db, "h1", "书一", 30, 1_100, 900, 0);

        let stats = merge_remote_progress(&db, &[cloud_progress("h1", 20, 1_050)]).unwrap();

        // 本地已是脏行且值全部保留：属于 no-change 跳过
        assert_eq!(stats.skipped, 1);
        let row: (i64, i64, i64) = db
            .query_row(
                "SELECT current_page, updated_at, synced_at FROM books WHERE hash = 'h1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        // 本地值胜出：行保持脏状态（synced_at 不动），等待下轮 push 上推本地页码
        assert_eq!(row, (30, 1_100, 900));
    }

    #[test]
    fn merge_progress_窗口外远端时间更新则远端获胜() {
        let db = memory_db();
        insert_book_row(&db, "h1", "书一", 10, 1_000, 900, 0);

        let stats = merge_remote_progress(&db, &[cloud_progress("h1", 40, 130_500)]).unwrap();

        assert_eq!(stats.updated, 1);
        let row: (i64, i64, i64) = db
            .query_row(
                "SELECT current_page, updated_at, synced_at FROM books WHERE hash = 'h1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, (40, 130_500, 130_500));
    }

    #[test]
    fn merge_progress_窗口外本地时间更新则保留本地() {
        let db = memory_db();
        insert_book_row(&db, "h1", "书一", 50, 200_000, 900, 0);

        let stats = merge_remote_progress(&db, &[cloud_progress("h1", 20, 10_000)]).unwrap();

        // 本地已是脏行且值全部保留：属于 no-change 跳过
        assert_eq!(stats.skipped, 1);
        let row: (i64, i64, i64) = db
            .query_row(
                "SELECT current_page, updated_at, synced_at FROM books WHERE hash = 'h1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        // 本地值胜出：行保持脏状态（synced_at 不动），等待下轮 push 上推本地页码
        assert_eq!(row, (50, 200_000, 900));
    }

    #[test]
    fn merge_progress_本地无书时跳过该进度() {
        let db = memory_db();

        let stats = merge_remote_progress(&db, &[cloud_progress("missing", 20, 10_000)]).unwrap();

        assert_eq!(
            stats,
            MergeStats {
                inserted: 0,
                updated: 0,
                skipped: 1
            }
        );
    }

    #[test]
    fn cursor_get_set_具备幂等覆盖行为() {
        let db = memory_db();

        assert_eq!(get_cursor(&db).unwrap(), None);
        set_cursor(&db, "cursor-a").unwrap();
        assert_eq!(get_cursor(&db).unwrap(), Some(String::from("cursor-a")));
        set_cursor(&db, "cursor-b").unwrap();
        assert_eq!(get_cursor(&db).unwrap(), Some(String::from("cursor-b")));
    }

    #[test]
    fn merge_remote_rows_重复回放第二次全部_skipped_且数据不变() {
        let db = memory_db();
        let book_rows = vec![cloud_book("h1", "远端新", 300, false)];
        let progress_rows = vec![cloud_progress("h1", 88, 500)];

        let first_books = merge_remote_books(&db, &book_rows, 0).unwrap();
        let first_progress = merge_remote_progress(&db, &progress_rows).unwrap();
        let snapshot: (String, i64, i64, i64, String) = db
            .query_row(
                "SELECT title, current_page, updated_at, synced_at, cloud_state FROM books WHERE hash = 'h1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();

        let second_books = merge_remote_books(&db, &book_rows, 0).unwrap();
        let second_progress = merge_remote_progress(&db, &progress_rows).unwrap();
        let snapshot_again: (String, i64, i64, i64, String) = db
            .query_row(
                "SELECT title, current_page, updated_at, synced_at, cloud_state FROM books WHERE hash = 'h1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();

        assert_eq!(first_books.inserted, 1);
        assert_eq!(first_progress.updated, 1);
        assert_eq!(
            second_books,
            MergeStats {
                inserted: 0,
                updated: 0,
                skipped: 1
            }
        );
        assert_eq!(
            second_progress,
            MergeStats {
                inserted: 0,
                updated: 0,
                skipped: 1
            }
        );
        assert_eq!(snapshot, snapshot_again);
    }
}
