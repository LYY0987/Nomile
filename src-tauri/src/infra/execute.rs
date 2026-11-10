//! 批量执行。
//!
//! 采用「两阶段 rename」：先全部改成同目录下的临时唯一名，再改成最终目标名。
//! 这一步换来三件事的确定性：
//! 1. A→B、B→A 这类互换名字不会互相占位；
//! 2. 仅改大小写（file.txt → File.txt）不会因「同目录同名」失败；
//! 3. 目标名恰好是本批次另一个文件的名字时，腾挪顺序不再成问题。
//!
//! 代价是 rename 次数翻倍（仍属元数据操作，1 万文件在毫秒级），换正确性是值得的。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

use uuid::Uuid;

use crate::error::Result;
use crate::infra::history::{HistoryStore, JournalLine};
use crate::model::{ConflictPolicy, ExecuteResult, FailedRename, OperationState};

pub struct RenameTarget {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub src_name: String,
    pub dst_name: String,
}

pub fn run(
    dir: &str,
    targets: Vec<RenameTarget>,
    policy: ConflictPolicy,
    rules_snapshot: Option<serde_json::Value>,
    store: &HistoryStore,
) -> Result<ExecuteResult> {
    let started = Instant::now();
    let total = targets.len();

    if total == 0 {
        return Ok(ExecuteResult {
            operation_id: String::new(),
            total: 0,
            renamed: 0,
            skipped: 0,
            failed: Vec::new(),
            state: OperationState::Completed,
            duration_ms: 0,
        });
    }

    let (mut manifest, mut journal) = store.begin(dir, total, rules_snapshot)?;

    let mut reserved: HashSet<String> = targets.iter().map(|t| path_key(&t.dst)).collect();
    let mut renamed = 0usize;
    let mut skipped = 0usize;
    let mut failed: Vec<FailedRename> = Vec::new();
    let mut staged: Vec<(usize, PathBuf, u64, i64)> = Vec::new();

    // ---------------- 阶段一：原文件 → 临时名
    for (i, target) in targets.iter().enumerate() {
        if !target.src.exists() {
            failed.push(FailedRename {
                src_name: target.src_name.clone(),
                dst_name: target.dst_name.clone(),
                reason: "源文件已不存在".into(),
            });
            let _ = journal.append(&JournalLine::Fail {
                src: target.src.to_string_lossy().to_string(),
                reason: "源文件已不存在".into(),
            });
            continue;
        }

        let tmp = temp_path(&target.src);
        match fs::rename(&target.src, &tmp) {
            Ok(_) => {
                let (size, mtime) = stat_of(&tmp);
                journal.append(&JournalLine::Stage1 {
                    src: target.src.to_string_lossy().to_string(),
                    tmp: tmp.to_string_lossy().to_string(),
                    size,
                    mtime,
                })?;
                staged.push((i, tmp, size, mtime));
            }
            Err(e) => {
                let reason = describe_io_error(&e);
                failed.push(FailedRename {
                    src_name: target.src_name.clone(),
                    dst_name: target.dst_name.clone(),
                    reason: reason.clone(),
                });
                let _ = journal.append(&JournalLine::Fail {
                    src: target.src.to_string_lossy().to_string(),
                    reason,
                });
            }
        }
    }

    // ---------------- 阶段二：临时名 → 最终目标名
    for (i, tmp, size, mtime) in staged {
        let target = &targets[i];
        let mut final_dst = target.dst.clone();

        if final_dst.exists() {
            match policy {
                ConflictPolicy::Skip => {
                    let _ = fs::rename(&tmp, &target.src);
                    skipped += 1;
                    continue;
                }
                ConflictPolicy::AutoSuffix => {
                    final_dst = unique_path(&final_dst, &mut reserved);
                }
            }
        }

        match fs::rename(&tmp, &final_dst) {
            Ok(_) => {
                journal.append(&JournalLine::Stage2 {
                    tmp: tmp.to_string_lossy().to_string(),
                    dst: final_dst.to_string_lossy().to_string(),
                    size,
                    mtime,
                })?;
                renamed += 1;
            }
            Err(e) => {
                // 尽量把文件放回原名，避免留下 "~nm-" 临时文件
                let _ = fs::rename(&tmp, &target.src);
                let reason = describe_io_error(&e);
                failed.push(FailedRename {
                    src_name: target.src_name.clone(),
                    dst_name: target.dst_name.clone(),
                    reason: reason.clone(),
                });
                let _ = journal.append(&JournalLine::Fail {
                    src: target.src.to_string_lossy().to_string(),
                    reason,
                });
            }
        }
    }

    journal.close();

    manifest.renamed = renamed;
    manifest.failed = failed.len();
    let state = if failed.is_empty() && skipped == 0 {
        OperationState::Completed
    } else {
        OperationState::Partial
    };
    store.finish(&mut manifest, state)?;

    Ok(ExecuteResult {
        operation_id: manifest.id,
        total,
        renamed,
        skipped,
        failed,
        state,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn path_key(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

fn temp_path(src: &Path) -> PathBuf {
    let parent = src.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let ext = src
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    parent.join(format!(
        "~nm-{}{}",
        &Uuid::new_v4().simple().to_string()[..12],
        ext
    ))
}

/// 目标已存在且策略为「自动加后缀」时，找一个不冲突的名字。
/// 除了查磁盘，还要避开本批次其它条目的目标名，否则可能撞上还没落位的兄弟。
fn unique_path(base: &Path, reserved: &mut HashSet<String>) -> PathBuf {
    let parent = base.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let ext = base
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    for n in 1..=9999 {
        let candidate = parent.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() && !reserved.contains(&path_key(&candidate)) {
            reserved.insert(path_key(&candidate));
            return candidate;
        }
    }

    let candidate = parent.join(format!(
        "{stem} ({}){ext}",
        &Uuid::new_v4().simple().to_string()[..6]
    ));
    reserved.insert(path_key(&candidate));
    candidate
}

fn stat_of(path: &Path) -> (u64, i64) {
    match fs::metadata(path) {
        Ok(meta) => (
            meta.len(),
            meta.modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        ),
        Err(_) => (0, 0),
    }
}

fn describe_io_error(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => "没有权限修改该文件（可能只读或被系统保护）".into(),
        std::io::ErrorKind::AlreadyExists => "目标文件已存在".into(),
        std::io::ErrorKind::NotFound => "文件已被移走或删除".into(),
        _ => e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_path_avoids_reserved() {
        let dir = std::env::temp_dir().join(format!("nomile-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let base = dir.join("a.txt");
        fs::write(&base, b"x").unwrap();

        let mut reserved = HashSet::new();
        let p1 = unique_path(&base, &mut reserved);
        assert_eq!(p1.file_name().unwrap(), "a (1).txt");
        // 第二次必须再往后走，不能与前一次重复
        let p2 = unique_path(&base, &mut reserved);
        assert_eq!(p2.file_name().unwrap(), "a (2).txt");
        assert_ne!(p1, p2);

        let _ = fs::remove_dir_all(&dir);
    }

    use crate::infra::history::HistoryStore;

    fn put(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    fn target(dir: &Path, src: &str, dst: &str) -> RenameTarget {
        RenameTarget {
            src: dir.join(src),
            dst: dir.join(dst),
            src_name: src.to_string(),
            dst_name: dst.to_string(),
        }
    }

    #[test]
    fn swap_names_then_undo_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let store = HistoryStore::new(root.join("_ops"));

        put(root, "a.txt", "AAA");
        put(root, "b.txt", "BBB");
        put(root, "c.txt", "CCC");

        // a→b, b→c, c→a：三个名字互相占位，只有两阶段 rename 才能做到
        let targets = vec![
            target(root, "a.txt", "b.txt"),
            target(root, "b.txt", "c.txt"),
            target(root, "c.txt", "a.txt"),
        ];

        let result = run(
            root.to_str().unwrap(),
            targets,
            ConflictPolicy::Skip,
            None,
            &store,
        )
        .unwrap();

        assert_eq!(result.renamed, 3);
        assert!(result.failed.is_empty(), "不应有失败项");
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "AAA");
        assert_eq!(fs::read_to_string(root.join("c.txt")).unwrap(), "BBB");
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "CCC");

        let undo = store.undo(&result.operation_id).unwrap();
        assert_eq!(undo.restored, 3);
        assert!(undo.failed.is_empty());

        // 撤销后内容必须各回各位，而不只是名字对上
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "AAA");
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "BBB");
        assert_eq!(fs::read_to_string(root.join("c.txt")).unwrap(), "CCC");
    }

    #[test]
    fn existing_target_is_not_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let store = HistoryStore::new(root.join("_ops"));

        put(root, "a.txt", "A");
        put(root, "keep.txt", "K");

        let targets = vec![target(root, "a.txt", "keep.txt")];
        let result = run(
            root.to_str().unwrap(),
            targets,
            ConflictPolicy::AutoSuffix,
            None,
            &store,
        )
        .unwrap();

        assert_eq!(result.renamed, 1);
        // 原有文件必须原封不动
        assert_eq!(fs::read_to_string(root.join("keep.txt")).unwrap(), "K");
        // 新文件退到带序号的名字
        assert_eq!(fs::read_to_string(root.join("keep (1).txt")).unwrap(), "A");
        assert!(!root.join("a.txt").exists());
    }

    #[test]
    fn skip_policy_keeps_source_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let store = HistoryStore::new(root.join("_ops"));

        put(root, "a.txt", "A");
        put(root, "keep.txt", "K");

        let targets = vec![target(root, "a.txt", "keep.txt")];
        let result = run(
            root.to_str().unwrap(),
            targets,
            ConflictPolicy::Skip,
            None,
            &store,
        )
        .unwrap();

        assert_eq!(result.renamed, 0);
        assert_eq!(result.skipped, 1);
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "A");
        assert_eq!(fs::read_to_string(root.join("keep.txt")).unwrap(), "K");
    }
}
