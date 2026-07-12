#![allow(dead_code)]
//! 同步抽象层：为 Supabase、WebDAV 等可插拔后端提供统一接口。
//! 冲突处理默认依赖 `updated_at` 的最后写入生效（LWW）约定，服务端游标由实现层映射到 `next_cursor`。

use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

/// 云端书籍元数据行，承载 books 表的同步字段。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CloudBook {
    /// 文件内容哈希，作为跨设备对齐书籍的稳定主键。
    pub sha256: String,
    /// 书籍标题，用于列表展示与冲突后的用户识别。
    pub title: String,
    /// 作者名，可为空以兼容缺失元数据的文件。
    pub author: Option<String>,
    /// 总页数，可为空以兼容尚未解析完成的书籍。
    pub page_count: Option<i64>,
    /// 原始文件字节数，用于配额统计与上传校验。
    pub file_size: i64,
    /// 封面对象键，可为空表示尚未上传或无封面。
    pub cover_key: Option<String>,
    /// 原文件对象键，可为空表示仅同步元数据未同步文件。
    pub file_key: Option<String>,
    /// 客户端写入时间戳，单位为 Unix 毫秒，具体时区转换由实现层负责。
    pub updated_at: i64,
    /// 软删除标记，用于跨端传播删除状态。
    pub deleted: bool,
}

/// 云端阅读进度行，承载 reading_progress 表的同步字段。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CloudProgress {
    /// 文件内容哈希，指向对应书籍的稳定主键。
    pub sha256: String,
    /// 当前阅读页码，用于恢复阅读位置。
    pub page: i64,
    /// 缩放模式，可为空以兼容旧端未记录该设置。
    pub zoom_mode: Option<String>,
    /// 视图模式，可为空以兼容不同阅读器布局能力。
    pub view_mode: Option<String>,
    /// 写入该进度的设备名，可为空以兼容匿名设备。
    pub device_name: Option<String>,
    /// 客户端写入时间戳，单位为 Unix 毫秒，供 LWW 决策使用。
    pub updated_at: i64,
}

/// 一页增量拉取结果，包含本次同步窗口内的书籍与进度变更。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PullPage {
    /// 本页返回的书籍元数据集合。
    pub books: Vec<CloudBook>,
    /// 本页返回的阅读进度集合。
    pub progress: Vec<CloudProgress>,
    /// 下一页游标，是对 `server_updated_at` 的不透明封装。
    pub next_cursor: Option<String>,
}

/// 带过期时间的签名地址，用于客户端直传或直下载对象存储。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SignedUrl {
    /// 可直接访问的临时 URL。
    pub url: String,
    /// URL 失效时间戳，单位为 Unix 毫秒。
    pub expires_at: i64,
}

/// 登录态快照，用于保存用户身份与令牌续期信息。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AuthSession {
    /// 当前登录用户的唯一标识。
    pub user_id: String,
    /// 用于访问受保护接口的访问令牌。
    pub access_token: String,
    /// 用于续期访问令牌的刷新令牌。
    pub refresh_token: String,
    /// 会话过期时间戳，单位为 Unix 毫秒。
    pub expires_at: i64,
}

/// 同步层统一错误，屏蔽具体后端或协议细节。
#[derive(Debug, Clone, PartialEq)]
pub enum SyncError {
    /// 当前会话无效或缺失，需要重新认证。
    Unauthorized,
    /// 网络链路或远端服务异常，附带可读错误信息。
    Network(String),
    /// 写入冲突或版本竞争失败，附带冲突上下文。
    Conflict(String),
    /// 用户已达到存储或请求配额上限。
    QuotaExceeded,
    /// 其他未归类错误，保留原始信息便于上层透传。
    Other(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::Network(message) => write!(f, "network error: {message}"),
            Self::Conflict(message) => write!(f, "conflict: {message}"),
            Self::QuotaExceeded => write!(f, "quota exceeded"),
            Self::Other(message) => write!(f, "{message}"),
        }
    }
}

impl Error for SyncError {}

/// 可插拔同步后端接口；当前保持同步签名，后续实现层可在内部用 Tokio 阻塞桥接异步网络调用，避免此处引入 `async-trait` 依赖。
pub trait SyncBackend {
    /// 用户输入邮箱密码主动登录时调用，返回新的认证会话。
    fn sign_in(&mut self, email: &str, password: &str) -> Result<AuthSession, SyncError>;

    /// 用户首次注册云同步账号时调用，返回已登录的认证会话。
    fn sign_up(&mut self, email: &str, password: &str) -> Result<AuthSession, SyncError>;

    /// 访问令牌接近过期或已过期时调用，用刷新令牌换取新会话。
    fn refresh(&mut self) -> Result<AuthSession, SyncError>;

    /// 用户主动退出登录时调用，用于清理远端与本地会话状态。
    fn sign_out(&mut self) -> Result<(), SyncError>;

    /// 本地书籍元数据有新增、修改或软删除时批量上推。
    fn push_books(&self, rows: &[CloudBook]) -> Result<(), SyncError>;

    /// 本地阅读进度变化后批量上推，供其他设备恢复阅读位置。
    fn push_progress(&self, rows: &[CloudProgress]) -> Result<(), SyncError>;

    /// 从给定游标之后增量拉取变更页，用于冷启动或周期性同步。
    fn pull_since(&self, cursor: Option<&str>, limit: u32) -> Result<PullPage, SyncError>;

    /// 上传对象前调用，申请带大小信息约束的临时上传地址。
    fn sign_upload_url(&self, object_key: &str, bytes: i64) -> Result<SignedUrl, SyncError>;

    /// 下载对象前调用，申请可直接读取的临时下载地址。
    fn sign_download_url(&self, object_key: &str) -> Result<SignedUrl, SyncError>;

    /// 用户确认销毁云端账号与同步数据时调用。
    fn delete_account(&mut self) -> Result<(), SyncError>;
}
