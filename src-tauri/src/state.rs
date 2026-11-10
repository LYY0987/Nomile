//! 应用内存状态：当前扫描会话 + 配置 + 操作日志位置。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::infra::config::ConfigStore;
use crate::infra::history::HistoryStore;
use crate::model::{FileEntry, MediaMeta};

/// 一次扫描的结果。条目只在 Rust 侧存一份，
/// preview / execute 靠 session_id 拿，避免上万条记录在 IPC 上反复搬运。
pub struct ScanSession {
    pub dir: String,
    pub entries: Arc<Vec<FileEntry>>,
    /// 元数据缓存：同一个文件在一次会话内只读一次盘
    pub meta_cache: Arc<Mutex<HashMap<u64, MediaMeta>>>,
}

pub struct AppState {
    pub sessions: Mutex<HashMap<String, ScanSession>>,
    pub config: ConfigStore,
    pub history: HistoryStore,
}

impl AppState {
    pub fn new(config: ConfigStore, history: HistoryStore) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            config,
            history,
        }
    }
}
