//! 目录扫描。负责把磁盘上的文件变成 `FileEntry` 列表，并按用户选择排序。

use std::cmp::Ordering;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

use crate::domain::name::split_full_name;
use crate::error::{AppError, Result};
use crate::model::{FileEntry, FileKind, ScanRequest, SortBy};

pub struct ScanOutput {
    pub entries: Vec<FileEntry>,
    pub skipped: u32,
    pub truncated: bool,
}

pub fn scan_dir(req: &ScanRequest) -> Result<ScanOutput> {
    // 用户直接点名了文件：只处理这些路径，不走目录遍历
    if let Some(files) = req.files.as_ref().filter(|f| !f.is_empty()) {
        return Ok(scan_selected(files, req));
    }

    let root = PathBuf::from(&req.dir);
    if !root.is_dir() {
        return Err(AppError::InvalidPath(format!(
            "不是有效目录：{}",
            req.dir
        )));
    }

    let max = req.max_entries.unwrap_or(200_000).max(1);
    let mut entries: Vec<FileEntry> = Vec::new();
    let mut skipped: u32 = 0;
    let mut truncated = false;
    let mut next_id: u64 = 1;

    let walker = if req.recursive {
        WalkDir::new(&root).follow_links(false)
    } else {
        WalkDir::new(&root).max_depth(1).follow_links(false)
    };

    for item in walker {
        let de = match item {
            Ok(d) => d,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        let path = de.path();
        if path == root.as_path() {
            continue;
        }
        if de.file_type().is_symlink() {
            // 不处理符号链接，避免改名后链接目标错乱
            continue;
        }

        let meta = match de.metadata() {
            Ok(m) => m,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let is_dir = meta.is_dir();
        if is_dir && !req.include_dirs {
            continue;
        }
        if !is_dir && !req.include_files {
            continue;
        }

        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => {
                skipped += 1;
                continue;
            }
        };

        if !req.include_hidden && is_hidden(&meta, &name) {
            continue;
        }

        let inc = req.include_pattern.trim();
        if !inc.is_empty() && !any_pattern_matches(inc, &name) {
            continue;
        }
        let exc = req.exclude_pattern.trim();
        if !exc.is_empty() && any_pattern_matches(exc, &name) {
            continue;
        }

        if entries.len() >= max {
            truncated = true;
            break;
        }

        let dir = match path.parent().and_then(|p| p.to_str()) {
            Some(d) => d.to_string(),
            None => {
                skipped += 1;
                continue;
            }
        };

        let (stem, ext) = split_full_name(&name, is_dir);
        let modified = to_millis(meta.modified().ok());
        let created = to_millis(meta.created().ok());
        let size = meta.len();
        let readonly = meta.permissions().readonly();

        entries.push(FileEntry {
            id: next_id,
            dir,
            name,
            stem,
            kind: FileKind::of(&ext, is_dir),
            ext,
            size,
            modified,
            created,
            is_dir,
            readonly,
            // 元数据按需读取（用户规则里用到了才读），扫描阶段一律为空
            meta: None,
        });
        next_id += 1;
    }

    sort_entries(&mut entries, req.sort_by, req.sort_desc);

    Ok(ScanOutput {
        entries,
        skipped,
        truncated,
    })
}

/// 直接按选中的路径建条目。这些是用户亲手挑的，所以不再做包含/排除/隐藏过滤，
/// 只保留排序和"文件是否还在"的检查。
fn scan_selected(paths: &[String], req: &ScanRequest) -> ScanOutput {
    let mut entries: Vec<FileEntry> = Vec::with_capacity(paths.len());
    let mut skipped: u32 = 0;
    let mut next_id: u64 = 1;

    for raw in paths {
        let path = PathBuf::from(raw);
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if meta.file_type().is_symlink() {
            skipped += 1;
            continue;
        }

        let is_dir = meta.is_dir();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => {
                skipped += 1;
                continue;
            }
        };
        let dir = match path.parent().and_then(|p| p.to_str()) {
            Some(d) => d.to_string(),
            None => {
                skipped += 1;
                continue;
            }
        };

        let (stem, ext) = split_full_name(&name, is_dir);

        entries.push(FileEntry {
            id: next_id,
            dir,
            name,
            stem,
            kind: FileKind::of(&ext, is_dir),
            ext,
            size: meta.len(),
            modified: to_millis(meta.modified().ok()),
            created: to_millis(meta.created().ok()),
            is_dir,
            readonly: meta.permissions().readonly(),
            meta: None,
        });
        next_id += 1;
    }

    sort_entries(&mut entries, req.sort_by, req.sort_desc);

    ScanOutput {
        entries,
        skipped,
        truncated: false,
    }
}

fn to_millis(t: Option<std::time::SystemTime>) -> i64 {
    t.and_then(|v| v.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(windows)]
fn is_hidden(meta: &std::fs::Metadata, name: &str) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 || name.starts_with('.')
}

#[cfg(not(windows))]
fn is_hidden(_meta: &std::fs::Metadata, name: &str) -> bool {
    name.starts_with('.')
}

fn sort_entries(entries: &mut [FileEntry], by: SortBy, desc: bool) {
    match by {
        SortBy::Name => entries.sort_by(|a, b| natural_cmp(&a.name, &b.name)),
        SortBy::Path => entries.sort_by(|a, b| {
            natural_cmp(&a.dir, &b.dir).then_with(|| natural_cmp(&a.name, &b.name))
        }),
        SortBy::Modified => entries.sort_by(|a, b| {
            a.modified
                .cmp(&b.modified)
                .then_with(|| natural_cmp(&a.name, &b.name))
        }),
        SortBy::Created => entries.sort_by(|a, b| {
            a.created
                .cmp(&b.created)
                .then_with(|| natural_cmp(&a.name, &b.name))
        }),
        SortBy::Size => entries.sort_by(|a, b| {
            a.size
                .cmp(&b.size)
                .then_with(|| natural_cmp(&a.name, &b.name))
        }),
    }
    if desc {
        entries.reverse();
    }
}

/// 自然排序：`file2` 排在 `file10` 前面
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let ta = tokenize(a);
    let tb = tokenize(b);

    for (x, y) in ta.iter().zip(tb.iter()) {
        let ord = match (x, y) {
            ((true, xs), (true, ys)) => match (xs.parse::<u128>(), ys.parse::<u128>()) {
                (Ok(n1), Ok(n2)) => n1.cmp(&n2),
                _ => xs.cmp(ys),
            },
            _ => x.1.to_ascii_lowercase().cmp(&y.1.to_ascii_lowercase()),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    ta.len().cmp(&tb.len())
}

fn tokenize(s: &str) -> Vec<(bool, String)> {
    let mut out: Vec<(bool, String)> = Vec::new();
    let mut buf = String::new();
    let mut digit: Option<bool> = None;

    for c in s.chars() {
        let d = c.is_ascii_digit();
        match digit {
            Some(prev) if prev == d => buf.push(c),
            Some(prev) => {
                out.push((prev, std::mem::take(&mut buf)));
                buf.push(c);
                digit = Some(d);
            }
            None => {
                buf.push(c);
                digit = Some(d);
            }
        }
    }
    if !buf.is_empty() {
        out.push((digit.unwrap_or(false), buf));
    }
    out
}

/// 过滤框里可以用逗号 / 分号 / 竖线写多个模式，命中任意一个就算匹配
fn any_pattern_matches(patterns: &str, name: &str) -> bool {
    patterns
        .split(|c: char| c == ',' || c == ';' || c == '|')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .any(|p| wildcard_match(p, name))
}

/// 通配符匹配：支持 `*`（任意长度）和 `?`（单字符），大小写不敏感。
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut dp = vec![vec![false; t.len() + 1]; p.len() + 1];
    dp[0][0] = true;

    for i in 1..=p.len() {
        if p[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=p.len() {
        for j in 1..=t.len() {
            dp[i][j] = match p[i - 1] {
                '*' => dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i - 1][j - 1],
                c => dp[i - 1][j - 1] && c.to_ascii_lowercase() == t[j - 1].to_ascii_lowercase(),
            };
        }
    }

    dp[p.len()][t.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_star() {
        assert!(wildcard_match("*.txt", "a.txt"));
        assert!(!wildcard_match("*.txt", "a.jpg"));
        assert!(wildcard_match("*.jpg", "A.JPG"));
    }

    #[test]
    fn wildcard_question() {
        assert!(wildcard_match("IMG_??.jpg", "IMG_01.jpg"));
        assert!(!wildcard_match("IMG_??.jpg", "IMG_001.jpg"));
    }

    #[test]
    fn wildcard_plain() {
        assert!(wildcard_match("", ""));
        assert!(!wildcard_match("abc", "abcd"));
        assert!(wildcard_match("*", "anything"));
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["file10.txt", "file2.txt", "file1.txt"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["file1.txt", "file2.txt", "file10.txt"]);
    }

    #[test]
    fn natural_cmp_case_insensitive_prefix() {
        assert_eq!(natural_cmp("Photo.JPG", "photo.jpg"), Ordering::Equal);
    }

    #[test]
    fn tokenize_splits_digits() {
        let t = tokenize("a12b3");
        assert_eq!(t.len(), 4);
        assert!(t[1].0);
    }

    /// 扫描面板上的每个条件都必须真的生效，否则界面看着能调、实际不筛
    #[test]
    fn scan_honours_all_filter_conditions() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        std::fs::write(root.join("a.jpg"), b"x").unwrap();
        std::fs::write(root.join("b.tmp"), b"x").unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub").join("c.jpg"), b"x").unwrap();

        let mut req = ScanRequest {
            dir: root.to_string_lossy().to_string(),
            recursive: false,
            include_files: true,
            include_dirs: false,
            files: None,
            include_hidden: false,
            include_pattern: String::new(),
            exclude_pattern: String::new(),
            sort_by: SortBy::Name,
            sort_desc: false,
            max_entries: None,
        };

        let names_of = |req: &ScanRequest| {
            let mut v: Vec<String> = scan_dir(req)
                .unwrap()
                .entries
                .into_iter()
                .map(|e| e.name)
                .collect();
            v.sort();
            v
        };

        // 关掉「含子目录」：sub 里的文件不该出现
        assert_eq!(names_of(&req), vec!["a.jpg", "b.tmp"]);

        // 开「含子目录」+ 只包含 *.jpg：c.jpg 进来，b.tmp 被挡掉
        req.recursive = true;
        req.include_pattern = "*.jpg".into();
        assert_eq!(names_of(&req), vec!["a.jpg", "c.jpg"]);

        // 只保留排除规则
        req.include_pattern = String::new();
        req.exclude_pattern = "*.tmp".into();
        assert_eq!(names_of(&req), vec!["a.jpg", "c.jpg"]);

        // 勾上「含文件夹」：sub 本身也成为一条
        req.exclude_pattern = String::new();
        req.include_dirs = true;
        assert_eq!(names_of(&req), vec!["a.jpg", "b.tmp", "c.jpg", "sub"]);

        // 降序：名字最大的排最前（这里不能用 names_of，它会把结果重新排序）
        req.sort_desc = true;
        let out = scan_dir(&req).unwrap();
        assert_eq!(out.entries.first().map(|e| e.name.as_str()), Some("sub"));
    }

    #[test]
    fn comma_separated_patterns() {
        assert!(any_pattern_matches("*.jpg, *.png", "a.jpg"));
        assert!(any_pattern_matches("*.jpg, *.png", "a.PNG"));
        assert!(!any_pattern_matches("*.jpg, *.png", "a.txt"));
        // 只有分隔符和空格时等于没写
        assert!(!any_pattern_matches(" , ; ", "a.txt"));
    }

    /// 直接点名文件：只处理这些，且不被范围条件误伤
    #[test]
    fn scan_selected_ignores_scope_filters() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("a.txt"), b"x").unwrap();
        std::fs::write(td.path().join("b.log"), b"x").unwrap();
        std::fs::write(td.path().join("not-picked.txt"), b"x").unwrap();

        let req = ScanRequest {
            dir: td.path().to_string_lossy().to_string(),
            files: Some(vec![
                td.path().join("b.log").to_string_lossy().to_string(),
                td.path().join("a.txt").to_string_lossy().to_string(),
            ]),
            recursive: true,
            include_files: true,
            include_dirs: false,
            include_hidden: false,
            // 范围条件故意写成只匹配 *.nothing：点名文件不该被它挡掉
            include_pattern: "*.nothing".into(),
            exclude_pattern: "*.log".into(),
            sort_by: SortBy::Name,
            sort_desc: false,
            max_entries: None,
        };

        let out = scan_dir(&req).unwrap();
        let names: Vec<&str> = out.entries.iter().map(|e| e.name.as_str()).collect();
        // 排序仍然生效，范围过滤不生效
        assert_eq!(names, vec!["a.txt", "b.log"]);
    }

    /// 上限截断：max_entries 生效时 truncated 必须为 true
    #[test]
    fn scan_truncates_at_max_entries() {
        let td = tempfile::tempdir().unwrap();
        for i in 0..5 {
            std::fs::write(td.path().join(format!("f{i}.txt")), b"x").unwrap();
        }
        let req = ScanRequest {
            dir: td.path().to_string_lossy().to_string(),
            recursive: false,
            include_files: true,
            include_dirs: false,
            files: None,
            include_hidden: false,
            include_pattern: String::new(),
            exclude_pattern: String::new(),
            sort_by: SortBy::Name,
            sort_desc: false,
            max_entries: Some(3),
        };
        let out = scan_dir(&req).unwrap();
        assert_eq!(out.entries.len(), 3);
        assert!(out.truncated);
    }
}
