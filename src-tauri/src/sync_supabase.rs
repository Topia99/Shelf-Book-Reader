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
    file_size: i64,
    cover_key: Option<String>,
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
            file_size: self.file_size,
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
