//! 按内容命名时读取文本摘要。只读常见文本文件，限制大小，避免把大文件或二进制塞进模型。

use std::fs::File;
use std::io::{Read, Take};
use std::path::Path;

const TEXT_EXTS: &[&str] = &[
    "txt", "md", "markdown", "csv", "tsv", "json", "yaml", "yml", "xml", "html", "htm", "css",
    "js", "mjs", "cjs", "ts", "tsx", "jsx", "py", "rs", "go", "java", "kt", "c", "h", "cpp",
    "hpp", "cs", "sh", "bat", "ps1", "log", "ini", "toml", "sql", "vue", "svelte", "tex",
];

/// 单文件最多读取的原始字节数
const MAX_BYTES: u64 = 256 * 1024;
/// 送给模型的摘要字符数上限
pub const MAX_CHARS: usize = 1500;

pub struct Snippet {
    pub text: String,
    pub truncated: bool,
}

pub fn supports(ext: &str) -> bool {
    TEXT_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

/// 读取文本摘要。返回 None 表示不适合读内容（过大、二进制、空文件）。
pub fn read_snippet(path: &Path, max_chars: usize) -> Result<Option<Snippet>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() == 0 || meta.len() > MAX_BYTES {
        return Ok(None);
    }

    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut reader: Take<File> = file.take(MAX_BYTES);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;

    // 含 NUL 的按二进制处理，不读内容
    if bytes.contains(&0) {
        return Ok(None);
    }

    let text = match String::from_utf8(bytes.clone()) {
        Ok(text) => text,
        Err(_) => String::from_utf8_lossy(&bytes).to_string(),
    };

    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Ok(None);
    }

    let total = normalized.chars().count();
    let snippet: String = normalized.chars().take(max_chars.max(1)).collect();
    Ok(Some(Snippet {
        text: snippet,
        truncated: total > max_chars,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_text_snippet() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        std::fs::write(&path, "hello\n\n  world  ").unwrap();
        let snippet = read_snippet(&path, 100).unwrap().unwrap();
        assert_eq!(snippet.text, "hello world");
        assert!(!snippet.truncated);
    }

    #[test]
    fn truncates_long_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("long.md");
        std::fs::write(&path, "abcdefghij".repeat(20)).unwrap();
        let snippet = read_snippet(&path, 30).unwrap().unwrap();
        assert_eq!(snippet.text.chars().count(), 30);
        assert!(snippet.truncated);
    }

    #[test]
    fn rejects_binary_and_empty() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("a.bin");
        std::fs::write(&binary, [0u8, 1, 2, 3]).unwrap();
        assert!(read_snippet(&binary, 100).unwrap().is_none());

        let empty = dir.path().join("empty.txt");
        std::fs::write(&empty, b"").unwrap();
        assert!(read_snippet(&empty, 100).unwrap().is_none());
    }

    #[test]
    fn supports_common_text_extensions() {
        assert!(supports("md"));
        assert!(supports("TXT"));
        assert!(!supports("jpg"));
        assert!(!supports("docx"));
    }
}
