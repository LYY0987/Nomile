//! 操作日志与撤销。
//!
//! 每次批量执行都会在 `%APPDATA%\Nomile\operations\<id>\` 下留一份：
//! - `manifest.json`：元信息与最终状态
//! - `journal.jsonl`：逐条预写日志（阶段一 → 临时名，阶段二 → 目标名）
//!
//! 崩溃后重启，读 journal 就能还原"每个文件现在在哪"，因此撤销不依赖内存状态。

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::Local;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::model::{OperationState, OperationSummary, UndoFailure, UndoResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub dir: String,
    pub started_at: i64,
    pub total: usize,
    pub renamed: usize,
    pub failed: usize,
    pub state: OperationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum JournalLine {
    Stage1 {
        src: String,
        tmp: String,
        size: u64,
        mtime: i64,
    },
    Stage2 {
        tmp: String,
        dst: String,
        size: u64,
        mtime: i64,
    },
    Fail {
        src: String,
        #[serde(default)]
        reason: String,
    },
}

pub struct JournalWriter {
    file: File,
    pending: u32,
}

impl JournalWriter {
    pub fn append(&mut self, line: &JournalLine) -> Result<()> {
        let mut text = serde_json::to_string(line)?;
        text.push('\n');
        self.file.write_all(text.as_bytes())?;
        self.file.flush()?;

        // 每 100 条真正刷盘一次：rename 本身是元数据操作，崩溃概率低，
        // 逐条 fsync 会把 1 万文件的开销放大到秒级。
        self.pending += 1;
        if self.pending >= 100 {
            let _ = self.file.sync_data();
            self.pending = 0;
        }
        Ok(())
    }

    pub fn close(self) {
        let _ = self.file.sync_data();
    }
}

pub struct HistoryStore {
    root: PathBuf,
}

impl HistoryStore {
    pub fn new(root: PathBuf) -> Self {
        let _ = fs::create_dir_all(&root);
        Self { root }
    }

    pub fn begin(
        &self,
        dir: &str,
        total: usize,
        rules: Option<serde_json::Value>,
    ) -> Result<(Manifest, JournalWriter)> {
        let now = Local::now();
        let id = format!(
            "{}-{}",
            now.format("%Y%m%d-%H%M%S"),
            &Uuid::new_v4().simple().to_string()[..6]
        );
        let op_dir = self.root.join(&id);
        fs::create_dir_all(&op_dir)?;

        let manifest = Manifest {
            id: id.clone(),
            dir: dir.to_string(),
            started_at: now.timestamp_millis(),
            total,
            renamed: 0,
            failed: 0,
            state: OperationState::Running,
            rules,
        };
        write_manifest(&op_dir, &manifest)?;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(op_dir.join("journal.jsonl"))?;

        Ok((
            manifest,
            JournalWriter {
                file,
                pending: 0,
            },
        ))
    }

    pub fn finish(&self, manifest: &mut Manifest, state: OperationState) -> Result<()> {
        manifest.state = state;
        write_manifest(&self.root.join(&manifest.id), manifest)
    }

    pub fn list(&self) -> Result<Vec<OperationSummary>> {
        let mut out = Vec::new();
        if !self.root.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let manifest_path = entry.path().join("manifest.json");
            if !manifest_path.exists() {
                continue;
            }
            let Ok(text) = fs::read_to_string(&manifest_path) else {
                continue;
            };
            let Ok(m) = serde_json::from_str::<Manifest>(&text) else {
                continue;
            };
            out.push(OperationSummary {
                can_undo: matches!(m.state, OperationState::Completed | OperationState::Partial)
                    && m.renamed > 0,
                id: m.id,
                dir: m.dir,
                started_at: m.started_at,
                total: m.total,
                renamed: m.renamed,
                failed: m.failed,
                state: m.state,
            });
        }
        out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(out)
    }

    pub fn undo(&self, id: &str) -> Result<UndoResult> {
        let op_dir = self.root.join(id);
        if !op_dir.join("manifest.json").exists() {
            return Err(AppError::Message(format!("找不到操作记录：{id}")));
        }

        let lines = read_journal(&op_dir)?;

        struct Item {
            src: String,
            current: String,
            size: u64,
            mtime: i64,
        }

        // 按日志重建每个条目当前所在位置
        let mut items: Vec<Item> = Vec::new();
        for line in &lines {
            match line {
                JournalLine::Stage1 {
                    src,
                    tmp,
                    size,
                    mtime,
                } => items.push(Item {
                    src: src.clone(),
                    current: tmp.clone(),
                    size: *size,
                    mtime: *mtime,
                }),
                JournalLine::Stage2 {
                    tmp,
                    dst,
                    size,
                    mtime,
                } => {
                    if let Some(item) = items.iter_mut().find(|it| &it.current == tmp) {
                        item.current = dst.clone();
                        item.size = *size;
                        item.mtime = *mtime;
                    }
                }
                JournalLine::Fail { .. } => {}
            }
        }

        let mut restored = 0usize;
        let mut skipped = 0usize;
        let mut failed: Vec<UndoFailure> = Vec::new();
        let mut staged: Vec<(usize, PathBuf)> = Vec::new();

        // 阶段 A：先全部搬到临时名，避免 A→B、B→A 这种互换名字时互相占位
        for (i, item) in items.iter().enumerate() {
            if item.current == item.src {
                skipped += 1;
                continue;
            }

            let current = PathBuf::from(&item.current);
            if !current.exists() {
                skipped += 1;
                continue;
            }

            match fs::metadata(&current) {
                Ok(meta) => {
                    let size = meta.len();
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    if size != item.size || (mtime - item.mtime).abs() > 2000 {
                        skipped += 1;
                        failed.push(UndoFailure {
                            name: item.current.clone(),
                            reason: "文件在执行后被修改过，已跳过以免错位".into(),
                        });
                        continue;
                    }
                }
                Err(e) => {
                    skipped += 1;
                    failed.push(UndoFailure {
                        name: item.current.clone(),
                        reason: e.to_string(),
                    });
                    continue;
                }
            }

            let tmp = undo_temp_path(&current);
            match fs::rename(&current, &tmp) {
                Ok(_) => staged.push((i, tmp)),
                Err(e) => {
                    skipped += 1;
                    failed.push(UndoFailure {
                        name: item.current.clone(),
                        reason: e.to_string(),
                    });
                }
            }
        }

        // 阶段 B：临时名 → 原名
        for (i, tmp) in staged.iter().rev() {
            let item = &items[*i];
            let target = PathBuf::from(&item.src);

            if target.exists() {
                failed.push(UndoFailure {
                    name: item.src.clone(),
                    reason: "原文件名已被别的文件占用".into(),
                });
                let _ = fs::rename(tmp, PathBuf::from(&item.current));
                continue;
            }

            match fs::rename(tmp, &target) {
                Ok(_) => restored += 1,
                Err(e) => {
                    failed.push(UndoFailure {
                        name: item.src.clone(),
                        reason: e.to_string(),
                    });
                    let _ = fs::rename(tmp, PathBuf::from(&item.current));
                }
            }
        }

        if let Ok(text) = fs::read_to_string(op_dir.join("manifest.json")) {
            if let Ok(mut m) = serde_json::from_str::<Manifest>(&text) {
                m.state = OperationState::RolledBack;
                write_manifest(&op_dir, &m)?;
            }
        }

        Ok(UndoResult {
            operation_id: id.to_string(),
            restored,
            skipped,
            failed,
        })
    }
}

fn read_journal(dir: &Path) -> Result<Vec<JournalLine>> {
    let path = dir.join("journal.jsonl");
    let mut out = Vec::new();
    if !path.exists() {
        return Ok(out);
    }
    for line in BufReader::new(File::open(&path)?).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        // 崩溃可能留下半行，跳过即可
        if let Ok(v) = serde_json::from_str::<JournalLine>(&line) {
            out.push(v);
        }
    }
    Ok(out)
}

fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<()> {
    let text = serde_json::to_string_pretty(manifest)?;
    let tmp = dir.join("manifest.json.tmp");
    fs::write(&tmp, text)?;
    let target = dir.join("manifest.json");
    if target.exists() {
        let _ = fs::remove_file(&target);
    }
    fs::rename(&tmp, &target)?;
    Ok(())
}

fn undo_temp_path(path: &Path) -> PathBuf {
    let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    parent.join(format!(
        "~nm-undo-{}{}",
        &Uuid::new_v4().simple().to_string()[..10],
        ext
    ))
}
