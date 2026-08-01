//! Supabase 同步后端的阻塞实现。
//!
//! 该实现只允许运行在专用 `std::thread` 中，绝不可在 Tauri 异步运行时线程内调用，
//! 否则会阻塞运行时工作线程并放大界面卡顿或死锁风险。

use crate::sync::{
    AuthSession, CloudBook, CloudProgress, PullPage, SignedUrl, SyncBackend, SyncError,
};
use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_CURSOR: &str = "1970-01-01T00:00:00Z";
const BODY_SUMMARY_LIMIT: usize = 160;

/// Supabase 的阻塞式同步后端。
pub struct SupabaseBackend {
    base_url: String,
    anon_key: String,
    client: Client,
    session: Option<AuthSession>,
}

impl SupabaseBackend {
    /// 创建新的 Supabase 后端实例。
    pub fn new(base_url: String, anon_key: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest blocking client build failed");

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            anon_key,
            client,
            session: None,
        }
    }

    /// 恢复历史会话（钥匙串加载后注入）；有效性由随后的 refresh 校验。
    pub fn set_session(&mut self, session: AuthSession) {
        self.session = Some(session);
    }

    /// 当前会话的只读视图（引擎用于判断登录态与过期时间）。
    pub fn session(&self) -> Option<&AuthSession> {
        self.session.as_ref()
    }

    fn auth_url(&self, path: &str) -> String {
        format!("{}/auth/v1{path}", self.base_url)
    }

    fn rest_url(&self, path: &str) -> String {
        format!("{}/rest/v1{path}", self.base_url)
    }

    fn function_url(&self, path: &str) -> String {
        format!("{}/functions/v1{path}", self.base_url)
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    fn apply_apikey(&self, request: RequestBuilder) -> RequestBuilder {
        request.header("apikey", &self.anon_key)
    }

    fn apply_auth(&self, request: RequestBuilder) -> Result<RequestBuilder, SyncError> {
        let session = self.session.as_ref().ok_or(SyncError::Unauthorized)?;
        Ok(self
            .apply_apikey(request)
            .header(AUTHORIZATION, format!("Bearer {}", session.access_token)))
    }

    fn send_json<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, SyncError> {
        let response = request.send().map_err(map_network_error)?;
        parse_json_response(response, map_http_error)
    }

    fn send_json_with_mapper<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
        error_mapper: fn(StatusCode, &str) -> SyncError,
    ) -> Result<T, SyncError> {
        let response = request.send().map_err(map_network_error)?;
        parse_json_response(response, error_mapper)
    }

    fn send_empty(&self, request: RequestBuilder) -> Result<(), SyncError> {
        let response = request.send().map_err(map_network_error)?;
        parse_empty_response(response, map_http_error)
    }

    fn send_empty_with_mapper(
        &self,
        request: RequestBuilder,
        error_mapper: fn(StatusCode, &str) -> SyncError,
    ) -> Result<(), SyncError> {
        let response = request.send().map_err(map_network_error)?;
        parse_empty_response(response, error_mapper)
    }

    fn store_session_from_response(
        &mut self,
        payload: AuthApiResponse,
        allow_missing_session: bool,
    ) -> Result<Option<AuthSession>, SyncError> {
        let Some(session) = payload.into_session() else {
            if allow_missing_session {
                return Ok(None);
            }
            return Err(SyncError::Other("认证响应缺少 session".to_string()));
        };

        self.session = Some(session.clone());
        Ok(Some(session))
    }
}

impl SyncBackend for SupabaseBackend {
    fn sign_in(&mut self, email: &str, password: &str) -> Result<AuthSession, SyncError> {
        let request = self.apply_apikey(
            self.client
                .post(self.auth_url("/token?grant_type=password"))
                .json(&json!({ "email": email, "password": password })),
        );

        let payload: AuthApiResponse = self.send_json(request)?;
        self.store_session_from_response(payload, false)?
            .ok_or_else(|| SyncError::Other("认证响应缺少 session".to_string()))
    }

    fn sign_up(&mut self, email: &str, password: &str) -> Result<AuthSession, SyncError> {
        let request = self.apply_apikey(
            self.client
                .post(self.auth_url("/signup"))
                .json(&json!({ "email": email, "password": password })),
        );

        let payload: AuthApiResponse = self.send_json(request)?;
        self.store_session_from_response(payload, false)?
            .ok_or_else(|| SyncError::Other("认证响应缺少 session".to_string()))
    }

    fn refresh(&mut self) -> Result<AuthSession, SyncError> {
        let refresh_token = self
            .session
            .as_ref()
            .map(|session| session.refresh_token.clone())
            .ok_or(SyncError::Unauthorized)?;

        let request = self.apply_apikey(
            self.client
                .post(self.auth_url("/token?grant_type=refresh_token"))
                .json(&json!({ "refresh_token": refresh_token })),
        );

        let payload: AuthApiResponse = self.send_json(request)?;
        self.store_session_from_response(payload, false)?
            .ok_or_else(|| SyncError::Other("认证响应缺少 session".to_string()))
    }

    fn sign_out(&mut self) -> Result<(), SyncError> {
        let request = self.apply_auth(self.client.post(self.auth_url("/logout")))?;
        self.send_empty(request)?;
        self.session = None;
        Ok(())
    }

    fn push_books(&self, rows: &[CloudBook]) -> Result<(), SyncError> {
        if rows.is_empty() {
            return Ok(());
        }

        let user_id = self
            .session
            .as_ref()
            .map(|session| session.user_id.clone())
            .ok_or(SyncError::Unauthorized)?;

        let payload: Result<Vec<_>, _> = rows
            .iter()
            .map(|row| {
                Ok(BookUpsertRow {
                    user_id: user_id.clone(),
                    sha256: row.sha256.clone(),
                    title: row.title.clone(),
                    author: row.author.clone(),
                    page_count: row.page_count,
                    file_size: row.file_size,
                    cover_key: row.cover_key.clone(),
                    file_key: row.file_key.clone(),
                    updated_at: unix_ms_to_rfc3339(row.updated_at)?,
                    deleted: row.deleted,
                })
            })
            .collect();

        let request = self.apply_auth(
            self.client
                .post(self.rest_url("/books?on_conflict=user_id,sha256"))
                .header("Prefer", "resolution=merge-duplicates")
                .json(&payload.map_err(SyncError::Other)?),
        )?;

        self.send_empty(request)
    }

    fn push_progress(&self, rows: &[CloudProgress]) -> Result<(), SyncError> {
        if rows.is_empty() {
            return Ok(());
        }

        let user_id = self
            .session
            .as_ref()
            .map(|session| session.user_id.clone())
            .ok_or(SyncError::Unauthorized)?;

        let payload: Result<Vec<_>, _> = rows
            .iter()
            .map(|row| {
                Ok(ProgressUpsertRow {
                    user_id: user_id.clone(),
                    sha256: row.sha256.clone(),
                    page: row.page,
                    zoom_mode: row.zoom_mode.clone(),
                    view_mode: row.view_mode.clone(),
                    device_name: row.device_name.clone(),
                    updated_at: unix_ms_to_rfc3339(row.updated_at)?,
                })
            })
            .collect();

        let request = self.apply_auth(
            self.client
                .post(self.rest_url("/reading_progress?on_conflict=user_id,sha256"))
                .header("Prefer", "resolution=merge-duplicates")
                .json(&payload.map_err(SyncError::Other)?),
        )?;

        self.send_empty(request)
    }

    fn pull_since(&self, cursor: Option<&str>, limit: u32) -> Result<PullPage, SyncError> {
        let cursor = cursor.unwrap_or(DEFAULT_CURSOR);
        let limit_str = limit.to_string();

        let books_request = self.apply_auth(
            self.client
                .get(self.rest_url("/books"))
                .query(&[
                    (
                        "select",
                        "sha256,title,author,page_count,file_size,cover_key,file_key,updated_at,deleted,server_updated_at",
                    ),
                    ("server_updated_at", &format!("gt.{cursor}")),
                    ("order", "server_updated_at.asc"),
                    ("limit", limit_str.as_str()),
                ]),
        )?;
        let progress_request =
            self.apply_auth(self.client.get(self.rest_url("/reading_progress")).query(&[
                (
                    "select",
                    "sha256,page,zoom_mode,view_mode,device_name,updated_at,server_updated_at",
                ),
                ("server_updated_at", &format!("gt.{cursor}")),
                ("order", "server_updated_at.asc"),
                ("limit", limit_str.as_str()),
            ]))?;

        let book_rows: Vec<BookPullRow> = self.send_json(books_request)?;
        let progress_rows: Vec<ProgressPullRow> = self.send_json(progress_request)?;

        let mut max_cursor: Option<String> = None;
        let mut books = Vec::with_capacity(book_rows.len());
        for row in book_rows {
            max_cursor = pick_later_cursor(max_cursor, row.server_updated_at.clone())?;
            books.push(row.try_into_cloud_book()?);
        }

        let mut progress = Vec::with_capacity(progress_rows.len());
        for row in progress_rows {
            max_cursor = pick_later_cursor(max_cursor, row.server_updated_at.clone())?;
            progress.push(row.try_into_cloud_progress()?);
        }

        let has_more = books.len() as u32 == limit || progress.len() as u32 == limit;
        Ok(PullPage {
            books,
            progress,
            next_cursor: if has_more { max_cursor } else { None },
        })
    }

    fn sign_upload_url(&self, object_key: &str, bytes: i64) -> Result<SignedUrl, SyncError> {
        let request = self.apply_auth(
            self.client
                .post(self.function_url("/sign-url"))
                .json(&json!({ "op": "put", "key": object_key, "bytes": bytes })),
        )?;

        let payload: SignedUrlResponse = self.send_json_with_mapper(request, map_sign_url_error)?;
        payload.try_into_signed_url()
    }

    fn sign_download_url(&self, object_key: &str) -> Result<SignedUrl, SyncError> {
        let request = self.apply_auth(
            self.client
                .post(self.function_url("/sign-url"))
                .json(&json!({ "op": "get", "key": object_key, "bytes": 0 })),
        )?;

        let payload: SignedUrlResponse = self.send_json_with_mapper(request, map_sign_url_error)?;
        payload.try_into_signed_url()
    }

    fn delete_account(&mut self) -> Result<(), SyncError> {
        let request = self.apply_auth(
            self.client
                .post(self.function_url("/delete-account"))
                .header(CONTENT_TYPE, "application/json")
                .body(""),
        )?;

        self.send_empty_with_mapper(request, map_http_error)?;
        self.session = None;
        Ok(())
    }
}

impl SupabaseBackend {
    /// 传给 sign-url 函数的对象键：books/<sha256>.pdf（内容寻址）。
    /// 函数内部再自行加 {user_id}/ 前缀成完整 R2 键，实现用户隔离——
    /// 所以这里**不能**带 user_id 前缀，否则正则校验失败返回 403。
    fn book_object_key(&self, sha256: &str) -> Result<String, SyncError> {
        // 仍要求已登录（apply_auth 也依赖会话）
        self.session.as_ref().ok_or(SyncError::Unauthorized)?;
        Ok(format!("books/{sha256}.pdf"))
    }

    /// 上传书籍文件本体：签发预签名 PUT → 直传 R2 → 回填云端 file_key/file_size。
    /// 配额超限时 sign_upload_url 直接返回 QuotaExceeded，调用方据此停止本轮上传。
    pub(crate) fn upload_book_file(&self, sha256: &str, bytes: &[u8]) -> Result<(), SyncError> {
        let key = self.book_object_key(sha256)?;
        let signed = self.sign_upload_url(&key, bytes.len() as i64)?;

        let resp = self
            .client
            .put(&signed.url)
            .body(bytes.to_vec())
            .send()
            .map_err(|e| SyncError::Network(format!("R2 上传失败：{e}")))?;
        if !resp.status().is_success() {
            return Err(SyncError::Network(format!("R2 上传返回 {}", resp.status())));
        }

        let user_id = self
            .session
            .as_ref()
            .map(|s| s.user_id.clone())
            .ok_or(SyncError::Unauthorized)?;
        let request = self.apply_auth(
            self.client
                .patch(self.rest_url(&format!(
                    "/books?user_id=eq.{user_id}&sha256=eq.{sha256}"
                )))
                .json(&json!({ "file_key": key, "file_size": bytes.len() as i64 })),
        )?;
        self.send_empty(request)
    }

    /// 下载书籍文件本体：推导对象键 → 签发预签名 GET → 拉取字节。
    /// 远端对象不存在（对方尚未传完）时返回可识别的 Other 错误。
    pub(crate) fn download_book_file(&self, sha256: &str) -> Result<Vec<u8>, SyncError> {
        let key = self.book_object_key(sha256)?;
        let signed = self.sign_download_url(&key)?;
        let resp = self
            .client
            .get(&signed.url)
            .send()
            .map_err(|e| SyncError::Network(format!("R2 下载失败：{e}")))?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Err(SyncError::Other("远端文件尚未就绪".into()));
        }
        if !resp.status().is_success() {
            return Err(SyncError::Network(format!("R2 下载返回 {}", resp.status())));
        }
        resp.bytes()
            .map(|b| b.to_vec())
            .map_err(|e| SyncError::Network(format!("R2 下载读取失败：{e}")))
    }

    /// 封面对象键：covers/<sha256>.jpg（函数内部再加 {user_id}/ 前缀，同 book_object_key）。
    fn cover_object_key(&self, sha256: &str) -> Result<String, SyncError> {
        self.session.as_ref().ok_or(SyncError::Unauthorized)?;
        Ok(format!("covers/{sha256}.jpg"))
    }

    /// 签发预签名 DELETE URL（内部辅助，供 delete_book_file 用）。
    fn sign_delete_url(&self, object_key: &str) -> Result<SignedUrl, SyncError> {
        let request = self.apply_auth(
            self.client
                .post(self.function_url("/sign-url"))
                .json(&json!({ "op": "delete", "key": object_key })),
        )?;

        let payload: SignedUrlResponse = self.send_json_with_mapper(request, map_sign_url_error)?;
        payload.try_into_signed_url()
    }

    /// 删除云端 R2 上的书籍文件与封面对象（墓碑同步后清理）。
    /// sign-url 函数 delete 分支会 HEAD 取大小回扣配额；R2 对已不存在的对象 DELETE 返回 204，天然幂等。
    pub(crate) fn delete_book_file(&self, sha256: &str) -> Result<(), SyncError> {
        for key in [self.book_object_key(sha256)?, self.cover_object_key(sha256)?] {
            let signed = self.sign_delete_url(&key)?;
            let resp = self
                .client
                .delete(&signed.url)
                .send()
                .map_err(|e| SyncError::Network(format!("R2 删除失败：{e}")))?;
            if !resp.status().is_success() && resp.status() != StatusCode::NOT_FOUND {
                return Err(SyncError::Network(format!("R2 删除返回 {}", resp.status())));
            }
        }
        Ok(())
    }

    /// 上传封面缩略图到 R2 并回填云端 books.cover_key。
    pub(crate) fn upload_cover_file(&self, sha256: &str, bytes: &[u8]) -> Result<(), SyncError> {
        let key = self.cover_object_key(sha256)?;
        let signed = self.sign_upload_url(&key, bytes.len() as i64)?;

        let resp = self
            .client
            .put(&signed.url)
            .body(bytes.to_vec())
            .send()
            .map_err(|e| SyncError::Network(format!("R2 封面上传失败：{e}")))?;
        if !resp.status().is_success() {
            return Err(SyncError::Network(format!(
                "R2 封面上传返回 {}",
                resp.status()
            )));
        }

        let user_id = self
            .session
            .as_ref()
            .map(|s| s.user_id.clone())
            .ok_or(SyncError::Unauthorized)?;
        let request = self.apply_auth(
            self.client
                .patch(self.rest_url(&format!(
                    "/books?user_id=eq.{user_id}&sha256=eq.{sha256}"
                )))
                .json(&json!({ "cover_key": key })),
        )?;
        self.send_empty(request)
    }

    /// 下载封面缩略图字节（远端无封面时返回可识别的 Other 错误）。
    pub(crate) fn download_cover_file(&self, sha256: &str) -> Result<Vec<u8>, SyncError> {
        let key = self.cover_object_key(sha256)?;
        let signed = self.sign_download_url(&key)?;
        let resp = self
            .client
            .get(&signed.url)
            .send()
            .map_err(|e| SyncError::Network(format!("R2 封面下载失败：{e}")))?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Err(SyncError::Other("远端封面尚未就绪".into()));
        }
        if !resp.status().is_success() {
            return Err(SyncError::Network(format!(
                "R2 封面下载返回 {}",
                resp.status()
            )));
        }
        resp.bytes()
            .map(|b| b.to_vec())
            .map_err(|e| SyncError::Network(format!("R2 封面下载读取失败：{e}")))
    }

    /// 查询当前用户云存储配额（已用/上限字节，供设置页展示）。
    pub(crate) fn get_quota(&self) -> Result<crate::sync::QuotaInfo, SyncError> {
        let request = self.apply_auth(
            self.client
                .get(self.rest_url("/user_quota"))
                .query(&[("select", "bytes_used,bytes_limit"), ("limit", "1")]),
        )?;
        let rows: Vec<crate::sync::QuotaInfo> = self.send_json(request)?;
        Ok(rows.into_iter().next().unwrap_or_default())
    }
}

#[derive(Debug, Deserialize)]
struct AuthApiResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    user: Option<AuthUser>,
    session: Option<AuthSessionPayload>,
}

impl AuthApiResponse {
    fn into_session(self) -> Option<AuthSession> {
        if let Some(session) = self.session {
            return session.into_auth_session();
        }

        let user_id = self.user?.id;
        Some(AuthSession {
            user_id,
            access_token: self.access_token?,
            refresh_token: self.refresh_token?,
            expires_at: SupabaseBackend::now_ms() + self.expires_in? * 1000,
        })
    }
}

#[derive(Debug, Deserialize)]
struct AuthSessionPayload {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    user: Option<AuthUser>,
}

impl AuthSessionPayload {
    fn into_auth_session(self) -> Option<AuthSession> {
        Some(AuthSession {
            user_id: self.user?.id,
            access_token: self.access_token?,
            refresh_token: self.refresh_token?,
            expires_at: SupabaseBackend::now_ms() + self.expires_in? * 1000,
        })
    }
}

#[derive(Debug, Deserialize)]
struct AuthUser {
    id: String,
}

#[derive(Debug, Serialize)]
struct BookUpsertRow {
    user_id: String,
    sha256: String,
    title: String,
    author: Option<String>,
    page_count: Option<i64>,
    // 元数据 push 恒不带 file_size/file_key/cover_key（collect_dirty 恒 None）：省略后
    // PostgREST merge-duplicates 不更新缺失列，避免把 upload_*_file 已 PATCH 的真实值抹掉。
    // file_size 尤其致命：抹回 0 会让配额触发器 SUM 归零（P0-A 回归）。
    #[serde(skip_serializing_if = "Option::is_none")]
    file_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cover_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_key: Option<String>,
    updated_at: String,
    deleted: bool,
}

#[derive(Debug, Serialize)]
struct ProgressUpsertRow {
    user_id: String,
    sha256: String,
    page: i64,
    zoom_mode: Option<String>,
    view_mode: Option<String>,
    device_name: Option<String>,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct BookPullRow {
    sha256: String,
    title: String,
    author: Option<String>,
    page_count: Option<i64>,
    file_size: i64,
    cover_key: Option<String>,
    file_key: Option<String>,
    updated_at: String,
    deleted: bool,
    server_updated_at: String,
}

impl BookPullRow {
    fn try_into_cloud_book(self) -> Result<CloudBook, SyncError> {
        Ok(CloudBook {
            sha256: self.sha256,
            title: self.title,
            author: self.author,
            page_count: self.page_count,
            file_size: Some(self.file_size),
            cover_key: self.cover_key,
            file_key: self.file_key,
            updated_at: rfc3339_to_unix_ms(&self.updated_at).map_err(SyncError::Other)?,
            deleted: self.deleted,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ProgressPullRow {
    sha256: String,
    page: i64,
    zoom_mode: Option<String>,
    view_mode: Option<String>,
    device_name: Option<String>,
    updated_at: String,
    server_updated_at: String,
}

impl ProgressPullRow {
    fn try_into_cloud_progress(self) -> Result<CloudProgress, SyncError> {
        Ok(CloudProgress {
            sha256: self.sha256,
            page: self.page,
            zoom_mode: self.zoom_mode,
            view_mode: self.view_mode,
            device_name: self.device_name,
            updated_at: rfc3339_to_unix_ms(&self.updated_at).map_err(SyncError::Other)?,
        })
    }
}

#[derive(Debug, Deserialize)]
struct SignedUrlResponse {
    url: String,
    expires_at: SignedExpiry,
}

impl SignedUrlResponse {
    fn try_into_signed_url(self) -> Result<SignedUrl, SyncError> {
        let expires_at = match self.expires_at {
            SignedExpiry::Millis(value) => value,
            SignedExpiry::Rfc3339(value) => rfc3339_to_unix_ms(&value).map_err(SyncError::Other)?,
        };

        Ok(SignedUrl {
            url: self.url,
            expires_at,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SignedExpiry {
    Millis(i64),
    Rfc3339(String),
}

fn parse_json_response<T: DeserializeOwned>(
    response: Response,
    error_mapper: fn(StatusCode, &str) -> SyncError,
) -> Result<T, SyncError> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(error_mapper(status, &body));
    }

    response
        .json()
        .map_err(|error| SyncError::Other(format!("响应 JSON 解析失败: {error}")))
}

fn parse_empty_response(
    response: Response,
    error_mapper: fn(StatusCode, &str) -> SyncError,
) -> Result<(), SyncError> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }

    let body = response.text().unwrap_or_default();
    Err(error_mapper(status, &body))
}

fn map_network_error(error: reqwest::Error) -> SyncError {
    SyncError::Network(error.to_string())
}

fn map_http_error(status: StatusCode, body: &str) -> SyncError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => SyncError::Unauthorized,
        _ => SyncError::Other(format!(
            "HTTP {}: {}",
            status.as_u16(),
            summarize_body(body)
        )),
    }
}

fn map_sign_url_error(status: StatusCode, body: &str) -> SyncError {
    match status {
        StatusCode::PAYLOAD_TOO_LARGE => SyncError::QuotaExceeded,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => SyncError::Unauthorized,
        _ => SyncError::Other(format!(
            "HTTP {}: {}",
            status.as_u16(),
            summarize_body(body)
        )),
    }
}

fn summarize_body(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let summary: String = chars.by_ref().take(BODY_SUMMARY_LIMIT).collect();
    if chars.next().is_some() {
        format!("{summary}...")
    } else if summary.is_empty() {
        "<empty>".to_string()
    } else {
        summary
    }
}

fn pick_later_cursor(
    current: Option<String>,
    candidate: String,
) -> Result<Option<String>, SyncError> {
    if let Some(existing) = current {
        let existing_ms = rfc3339_to_unix_ms(&existing).map_err(SyncError::Other)?;
        let candidate_ms = rfc3339_to_unix_ms(&candidate).map_err(SyncError::Other)?;
        if candidate_ms >= existing_ms {
            Ok(Some(candidate))
        } else {
            Ok(Some(existing))
        }
    } else {
        Ok(Some(candidate))
    }
}

fn unix_ms_to_rfc3339(ms: i64) -> Result<String, String> {
    let seconds = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let second_of_day = seconds.rem_euclid(86_400);

    let (year, month, day) = date_from_days_since_epoch(days)?;
    let hour = second_of_day / 3_600;
    let minute = (second_of_day % 3_600) / 60;
    let second = second_of_day % 60;

    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z"
    ))
}

fn rfc3339_to_unix_ms(input: &str) -> Result<i64, String> {
    let (datetime, offset_seconds) = split_rfc3339_offset(input)?;
    let (date_part, time_part) = datetime
        .split_once('T')
        .ok_or_else(|| "RFC3339 缺少 T 分隔符".to_string())?;

    let mut date_iter = date_part.split('-');
    let year = parse_i32(date_iter.next(), "year")?;
    let month = parse_u32(date_iter.next(), "month")?;
    let day = parse_u32(date_iter.next(), "day")?;
    if date_iter.next().is_some() {
        return Err("RFC3339 日期部分格式非法".to_string());
    }

    let mut time_iter = time_part.split(':');
    let hour = parse_u32(time_iter.next(), "hour")?;
    let minute = parse_u32(time_iter.next(), "minute")?;
    let second_and_fraction = time_iter
        .next()
        .ok_or_else(|| "RFC3339 时间部分缺少秒".to_string())?;
    if time_iter.next().is_some() {
        return Err("RFC3339 时间部分格式非法".to_string());
    }

    let (second_str, fraction_str) = match second_and_fraction.split_once('.') {
        Some((second, fraction)) => (second, Some(fraction)),
        None => (second_and_fraction, None),
    };
    let second = second_str
        .parse::<u32>()
        .map_err(|_| "RFC3339 秒字段非法".to_string())?;

    if !(1..=12).contains(&month) {
        return Err("RFC3339 月份越界".to_string());
    }
    let max_day = days_in_month(year, month);
    if day == 0 || day > max_day {
        return Err("RFC3339 日期越界".to_string());
    }
    if hour > 23 || minute > 59 || second > 59 {
        return Err("RFC3339 时间越界".to_string());
    }

    let millis = parse_fraction_to_millis(fraction_str)?;
    let days = days_since_epoch_from_date(year, month, day)?;
    let day_ms = days
        .checked_mul(86_400_000)
        .ok_or_else(|| "时间戳溢出".to_string())?;
    let clock_ms = i64::from(hour) * 3_600_000
        + i64::from(minute) * 60_000
        + i64::from(second) * 1_000
        + i64::from(millis);

    day_ms
        .checked_add(clock_ms)
        .and_then(|value| value.checked_sub(i64::from(offset_seconds) * 1_000))
        .ok_or_else(|| "时间戳溢出".to_string())
}

fn split_rfc3339_offset(input: &str) -> Result<(&str, i32), String> {
    if let Some(stripped) = input.strip_suffix('Z') {
        return Ok((stripped, 0));
    }

    if input.len() < 6 {
        return Err("RFC3339 缺少时区信息".to_string());
    }

    let sign_index = input.len() - 6;
    let sign = input
        .as_bytes()
        .get(sign_index)
        .copied()
        .ok_or_else(|| "RFC3339 时区格式非法".to_string())?;
    if sign != b'+' && sign != b'-' {
        return Err("RFC3339 仅支持 Z 或 ±HH:MM 时区".to_string());
    }
    if input.as_bytes().get(input.len() - 3) != Some(&b':') {
        return Err("RFC3339 时区格式非法".to_string());
    }

    let offset = &input[sign_index + 1..];
    let (hour_str, minute_str) = offset
        .split_once(':')
        .ok_or_else(|| "RFC3339 时区格式非法".to_string())?;
    let hour = hour_str
        .parse::<i32>()
        .map_err(|_| "RFC3339 时区小时非法".to_string())?;
    let minute = minute_str
        .parse::<i32>()
        .map_err(|_| "RFC3339 时区分钟非法".to_string())?;
    if hour > 23 || minute > 59 {
        return Err("RFC3339 时区越界".to_string());
    }

    let total_seconds = hour * 3_600 + minute * 60;
    let signed_seconds = if sign == b'+' {
        total_seconds
    } else {
        -total_seconds
    };

    Ok((&input[..sign_index], signed_seconds))
}

fn parse_fraction_to_millis(fraction: Option<&str>) -> Result<u32, String> {
    let Some(fraction) = fraction else {
        return Ok(0);
    };
    if fraction.is_empty() || !fraction.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("RFC3339 毫秒部分非法".to_string());
    }

    let mut millis = String::with_capacity(3);
    for ch in fraction.chars().take(3) {
        millis.push(ch);
    }
    while millis.len() < 3 {
        millis.push('0');
    }

    millis
        .parse::<u32>()
        .map_err(|_| "RFC3339 毫秒部分非法".to_string())
}

fn parse_i32(value: Option<&str>, field: &str) -> Result<i32, String> {
    value
        .ok_or_else(|| format!("RFC3339 缺少 {field}"))
        .and_then(|segment| {
            segment
                .parse::<i32>()
                .map_err(|_| format!("RFC3339 {field} 字段非法"))
        })
}

fn parse_u32(value: Option<&str>, field: &str) -> Result<u32, String> {
    value
        .ok_or_else(|| format!("RFC3339 缺少 {field}"))
        .and_then(|segment| {
            segment
                .parse::<u32>()
                .map_err(|_| format!("RFC3339 {field} 字段非法"))
        })
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_since_epoch_from_date(year: i32, month: u32, day: u32) -> Result<i64, String> {
    let mut days = 0_i64;

    if year >= 1970 {
        for current_year in 1970..year {
            days += i64::from(days_in_year(current_year));
        }
    } else {
        for current_year in year..1970 {
            days -= i64::from(days_in_year(current_year));
        }
    }

    for current_month in 1..month {
        days += i64::from(days_in_month(year, current_month));
    }

    days.checked_add(i64::from(day) - 1)
        .ok_or_else(|| "时间戳溢出".to_string())
}

fn date_from_days_since_epoch(days: i64) -> Result<(i32, u32, u32), String> {
    let mut remaining_days = days;
    let mut year = 1970_i32;

    if remaining_days >= 0 {
        loop {
            let year_days = i64::from(days_in_year(year));
            if remaining_days < year_days {
                break;
            }
            remaining_days -= year_days;
            year = year
                .checked_add(1)
                .ok_or_else(|| "时间戳溢出".to_string())?;
        }
    } else {
        loop {
            let previous_year = year
                .checked_sub(1)
                .ok_or_else(|| "时间戳溢出".to_string())?;
            let year_days = i64::from(days_in_year(previous_year));
            remaining_days += year_days;
            year = previous_year;
            if remaining_days >= 0 {
                break;
            }
        }
    }

    let mut month = 1_u32;
    loop {
        let month_days = i64::from(days_in_month(year, month));
        if remaining_days < month_days {
            break;
        }
        remaining_days -= month_days;
        month += 1;
    }

    let day = u32::try_from(remaining_days + 1).map_err(|_| "时间戳溢出".to_string())?;
    Ok((year, month, day))
}

fn days_in_year(year: i32) -> u32 {
    if is_leap_year(year) {
        366
    } else {
        365
    }
}

#[cfg(test)]
mod tests {
    use super::{
        map_http_error, map_sign_url_error, rfc3339_to_unix_ms, unix_ms_to_rfc3339, SyncError,
    };
    use reqwest::StatusCode;

    /// 真实 dev Supabase + R2 端到端往返（P5 文件同步验证）。默认忽略，手动跑：
    /// SHELF_SUPABASE_URL=.. SHELF_SUPABASE_ANON_KEY=.. SHELF_TEST_EMAIL=.. SHELF_TEST_PASSWORD=.. \
    ///   cargo test --manifest-path src-tauri/Cargo.toml real_r2_roundtrip -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_r2_roundtrip() {
        use super::SupabaseBackend;
        use crate::sync::{CloudBook, SyncBackend};

        let url = std::env::var("SHELF_SUPABASE_URL").expect("SHELF_SUPABASE_URL");
        let key = std::env::var("SHELF_SUPABASE_ANON_KEY").expect("SHELF_SUPABASE_ANON_KEY");
        let email = std::env::var("SHELF_TEST_EMAIL").expect("SHELF_TEST_EMAIL");
        let password = std::env::var("SHELF_TEST_PASSWORD").expect("SHELF_TEST_PASSWORD");

        let mut backend = SupabaseBackend::new(url, key);
        backend.sign_in(&email, &password).expect("登录失败");

        let bytes = b"Shelf P5 end-to-end file-sync test payload".to_vec();
        let hash = crate::sha256_of_bytes(&bytes);
        let now = crate::now_ms();

        // 云端书行需先存在，upload_book_file 才能 PATCH 回填 file_key
        backend
            .push_books(&[CloudBook {
                sha256: hash.clone(),
                title: "P5 测试书".into(),
                author: None,
                page_count: None,
                // 元数据 push 不带 file_size（真实大小由 upload_book_file 的 PATCH 回填）
                file_size: None,
                cover_key: None,
                file_key: None,
                updated_at: now,
                deleted: false,
            }])
            .expect("push_books 失败");

        backend.upload_book_file(&hash, &bytes).expect("upload 失败");
        eprintln!("[test] 上传成功：hash={hash} {} 字节", bytes.len());

        let got = backend.download_book_file(&hash).expect("download 失败");
        assert_eq!(got, bytes, "下载字节应与上传一致");
        eprintln!("[test] 文件下载校验通过：{} 字节，内容一致 ✓", got.len());

        // P5-2：封面往返（covers/<hash>.jpg 走 COVER_KEY_RE）
        let cover = b"\xff\xd8\xff\xe0 fake-jpeg cover bytes for P5-2".to_vec();
        backend
            .upload_cover_file(&hash, &cover)
            .expect("upload_cover 失败");
        eprintln!("[test] 封面上传成功：{} 字节", cover.len());
        let got_cover = backend
            .download_cover_file(&hash)
            .expect("download_cover 失败");
        assert_eq!(got_cover, cover, "封面下载字节应与上传一致");
        eprintln!("[test] 封面下载校验通过：{} 字节，内容一致 ✓", got_cover.len());
    }

    /// P5-4 配额记账正确性：注册全新账号（干净基线），跑 5 个曾出问题的场景，
    /// 每步比对「实测配额 vs 期望」。期望值按「按实际存的文件大小之和」的正确模型算。
    /// 改前：预扣式计数 → 多个场景实测虚高（红）；改后：触发器重算 → 全部相符（绿）。
    #[test]
    #[ignore]
    fn quota_accounting_five_scenarios() {
        use super::SupabaseBackend;
        use crate::sync::{CloudBook, SyncBackend};

        let url = std::env::var("SHELF_SUPABASE_URL").expect("SHELF_SUPABASE_URL");
        let key = std::env::var("SHELF_SUPABASE_ANON_KEY").expect("SHELF_SUPABASE_ANON_KEY");

        let mut backend = SupabaseBackend::new(url, key);
        // 全新账号 → 干净配额基线（不受历史测试污染）
        let email = format!("shelf-quota-{}@gmail.com", crate::now_ms());
        let password = format!("Qt-{}-pw!", crate::now_ms());
        backend.sign_up(&email, &password).expect("注册失败");
        eprintln!("[quota-test] 全新账号 {email}");

        let used = |b: &SupabaseBackend| b.get_quota().expect("get_quota").bytes_used;

        // 书 A：文件 + 封面
        let data_a = b"quota scenario file A payload -- fixed length bytes".to_vec();
        let hash_a = crate::sha256_of_bytes(&data_a);
        let cover_a = b"\xff\xd8\xff\xe0 cover A jpeg-ish bytes".to_vec();
        let flen = data_a.len() as i64;
        let mk_book = |deleted: bool| CloudBook {
            sha256: hash_a.clone(),
            title: "配额测试书A".into(),
            author: None,
            page_count: None,
            // 模拟真实元数据 push：恒不带 file_size（否则会抹掉云端已记的真实大小）
            file_size: None,
            cover_key: None,
            file_key: None,
            updated_at: crate::now_ms(),
            deleted,
        };

        // 收集 (场景名, 实测, 期望)，跑完统一打印+断言（不中途停，好看到全部 5 个）
        let mut results: Vec<(&str, i64, i64)> = Vec::new();

        results.push(("基线：新账号", used(&backend), 0));

        // 元数据先入云端行（file_key 仍空，不该计入）
        backend.push_books(&[mk_book(false)]).expect("push A");
        results.push(("push 元数据(未传文件)", used(&backend), 0));

        // 场景1：签了上传地址但从不真传 → 不该计入
        let phantom = b"phantom never-uploaded object".to_vec();
        backend
            .sign_upload_url(&format!("books/{}.pdf", crate::sha256_of_bytes(&phantom)), 1000)
            .expect("sign put");
        results.push(("场景1 签了没传", used(&backend), 0));

        // 场景2：真上传一次 → 配额 = 文件大小
        backend.upload_book_file(&hash_a, &data_a).expect("upload A");
        results.push(("场景2 上传一次", used(&backend), flen));

        // 场景4：重传同一文件 → 配额不翻倍
        backend.upload_book_file(&hash_a, &data_a).expect("re-upload A");
        results.push(("场景4 重传同文件", used(&backend), flen));

        // 场景6（P0-A 回归守卫）：上传后再 push 元数据（模拟翻页 bump updated_at）
        // → 绝不能抹掉云端 file_size，配额必须仍等于 flen。
        // 改前 BookUpsertRow.file_size 无 skip_serializing_if，此处会把 file_size 覆盖成 0 → 配额归零。
        backend.push_books(&[mk_book(false)]).expect("re-push meta");
        results.push(("场景6 传后再push元数据", used(&backend), flen));

        // 场景3：上传封面 → 封面不计入配额（file_size 只记正文）
        backend.upload_cover_file(&hash_a, &cover_a).expect("upload cover A");
        results.push(("场景3 上传封面", used(&backend), flen));
        backend.upload_cover_file(&hash_a, &cover_a).expect("re-upload cover A");
        results.push(("场景3b 封面重传", used(&backend), flen));

        // 场景5：删除（推墓碑 deleted=true）→ 配额回落到 0
        backend.push_books(&[mk_book(true)]).expect("push deleted");
        results.push(("场景5 删除回落", used(&backend), 0));

        eprintln!("[quota-test] 结果（实测 / 期望）：");
        let mut bad = 0;
        for (name, obs, exp) in &results {
            let ok = obs == exp;
            if !ok {
                bad += 1;
            }
            eprintln!("  {} {name}: 实测={obs} 期望={exp}", if ok { "✓" } else { "✗" });
        }
        assert_eq!(bad, 0, "{bad} 个场景配额不符（见上方 ✗）");
        eprintln!("[quota-test] 5 场景全部通过 ✓");
    }

    #[test]
    fn unix_ms_to_rfc3339_handles_epoch_boundary_and_zero_padding() {
        let actual = unix_ms_to_rfc3339(7).expect("format epoch");
        assert_eq!(actual, "1970-01-01T00:00:00.007Z");
    }

    #[test]
    fn unix_ms_to_rfc3339_handles_leap_day() {
        let actual = unix_ms_to_rfc3339(1_709_164_800_045).expect("format leap day");
        assert_eq!(actual, "2024-02-29T00:00:00.045Z");
    }

    #[test]
    fn rfc3339_to_unix_ms_round_trips_epoch_boundary_leap_day_and_padding() {
        assert_eq!(rfc3339_to_unix_ms("1970-01-01T00:00:00Z").unwrap(), 0);
        assert_eq!(
            rfc3339_to_unix_ms("2024-02-29T00:00:00.045Z").unwrap(),
            1_709_164_800_045
        );
        assert_eq!(rfc3339_to_unix_ms("1970-01-01T00:00:00.7Z").unwrap(), 700);
    }

    #[test]
    fn rfc3339_conversions_handle_common_year_and_leap_year_end_boundaries() {
        assert_eq!(
            unix_ms_to_rfc3339(1_677_542_400_000).unwrap(),
            "2023-02-28T00:00:00.000Z"
        );
        assert_eq!(
            rfc3339_to_unix_ms("2023-02-28T00:00:00Z").unwrap(),
            1_677_542_400_000
        );
        assert_eq!(
            unix_ms_to_rfc3339(1_735_603_200_000).unwrap(),
            "2024-12-31T00:00:00.000Z"
        );
        assert_eq!(
            rfc3339_to_unix_ms("2024-12-31T00:00:00Z").unwrap(),
            1_735_603_200_000
        );
    }

    #[test]
    fn error_mapper_maps_unauthorized_and_quota() {
        assert_eq!(
            map_http_error(StatusCode::UNAUTHORIZED, "denied"),
            SyncError::Unauthorized
        );
        assert_eq!(
            map_sign_url_error(StatusCode::PAYLOAD_TOO_LARGE, "quota"),
            SyncError::QuotaExceeded
        );
    }

    #[test]
    fn error_mapper_wraps_other_status_with_summary() {
        let error = map_http_error(StatusCode::BAD_REQUEST, "bad\nrequest");
        assert_eq!(error, SyncError::Other("HTTP 400: bad request".to_string()));
    }
}
