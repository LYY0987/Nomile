//! 统一的错误类型。序列化成字符串回给前端，前端只做展示。

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("磁盘读写失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("数据格式错误：{0}")]
    Serde(#[from] serde_json::Error),

    #[error("路径无效：{0}")]
    InvalidPath(String),

    #[error("会话已失效，请重新扫描目录")]
    SessionMissing,

    #[error("{0}")]
    Message(String),
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl From<walkdir::Error> for AppError {
    fn from(e: walkdir::Error) -> Self {
        AppError::Message(format!("遍历目录失败：{e}"))
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
