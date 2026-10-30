use std::fmt;

/// Application-wide error type. Commands translate this into a user-facing
/// `String` at the Tauri boundary via `Into<String>`.
#[derive(Debug)]
pub enum AppError {
    Db(String),
    Pool(String),
    Network(String),
    Parse(String),
    NotFound(String),
    Invalid(String),
    Io(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Db(m) => write!(f, "数据库错误：{m}"),
            AppError::Pool(m) => write!(f, "连接池错误：{m}"),
            AppError::Network(m) => write!(f, "网络错误：{m}"),
            AppError::Parse(m) => write!(f, "解析错误：{m}"),
            AppError::NotFound(m) => write!(f, "未找到：{m}"),
            AppError::Invalid(m) => write!(f, "无效输入：{m}"),
            AppError::Io(m) => write!(f, "文件错误：{m}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e.to_string())
    }
}

impl From<r2d2::Error> for AppError {
    fn from(e: r2d2::Error) -> Self {
        AppError::Pool(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Network(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<AppError> for String {
    fn from(e: AppError) -> Self {
        e.to_string()
    }
}
