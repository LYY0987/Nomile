//! 文件名的拆分、校验与归一化。纯函数，无 IO。

use unicode_normalization::UnicodeNormalization;

/// Windows 明确禁止的字符
const ILLEGAL_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Windows 保留设备名（不区分大小写，带扩展名同样保留，例如 CON.txt）
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Windows 上单条路径的安全上限（未开启长路径支持时）
pub const MAX_PATH: usize = 260;

/// 把完整文件名拆成 (stem, ext)。目录没有扩展名概念。
/// `.gitignore` 这类点开头的文件视为无扩展名。
pub fn split_full_name(full: &str, is_dir: bool) -> (String, String) {
    if is_dir {
        return (full.to_string(), String::new());
    }
    match full.rfind('.') {
        Some(i) if i > 0 && i + 1 < full.len() => {
            (full[..i].to_string(), full[i + 1..].to_string())
        }
        _ => (full.to_string(), String::new()),
    }
}

pub fn compose(stem: &str, ext: &str) -> String {
    if ext.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}.{ext}")
    }
}

pub fn is_illegal(c: char) -> bool {
    ILLEGAL_CHARS.contains(&c) || (c as u32) < 32 || c == '\u{7f}'
}

pub fn first_illegal(name: &str) -> Option<char> {
    name.chars().find(|c| is_illegal(*c))
}

/// 判断 stem 是否命中 Windows 保留设备名
pub fn is_reserved(stem: &str) -> bool {
    let trimmed = stem.trim_end_matches([' ', '.']);
    let head = trimmed.split('.').next().unwrap_or(trimmed).to_uppercase();
    RESERVED_NAMES.contains(&head.as_str())
}

/// Windows 路径长度限制按 UTF-16 计算，这里做近似判断
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// 把文件名归一到 NFC，避免 macOS 拷来的 NFD 名字出现视觉相同但不相等的情况
pub fn normalize_nfc(s: &str) -> String {
    s.nfc().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_regular_file() {
        assert_eq!(
            split_full_name("IMG_001.jpg", false),
            ("IMG_001".into(), "jpg".into())
        );
    }

    #[test]
    fn split_dotfile_has_no_ext() {
        assert_eq!(
            split_full_name(".gitignore", false),
            (".gitignore".into(), "".into())
        );
    }

    #[test]
    fn split_trailing_dot() {
        assert_eq!(
            split_full_name("file.", false),
            ("file.".into(), "".into())
        );
    }

    #[test]
    fn split_dir_has_no_ext() {
        assert_eq!(
            split_full_name("my.folder", true),
            ("my.folder".into(), "".into())
        );
    }

    #[test]
    fn split_multi_dot() {
        assert_eq!(
            split_full_name("a.b.c.txt", false),
            ("a.b.c".into(), "txt".into())
        );
    }

    #[test]
    fn compose_roundtrip() {
        assert_eq!(compose("a", "txt"), "a.txt");
        assert_eq!(compose("a", ""), "a");
    }

    #[test]
    fn illegal_chars_detected() {
        assert_eq!(first_illegal("a:b"), Some(':'));
        assert_eq!(first_illegal("正常名字_2023"), None);
        assert!(is_illegal('\u{1}'));
    }

    #[test]
    fn reserved_names_detected() {
        assert!(is_reserved("CON"));
        assert!(is_reserved("con"));
        assert!(is_reserved("CON.txt"));
        assert!(!is_reserved("CONSOLE"));
        assert!(!is_reserved("com10"));
    }

    #[test]
    fn nfc_normalization() {
        let nfd = "e\u{301}"; // e + 组合重音
        let nfc = "é";
        assert_eq!(normalize_nfc(nfd), nfc);
    }
}
