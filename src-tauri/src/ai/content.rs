//! 按内容命名时读取内容摘要。
//!
//! 文本文件直接读；PDF / DOCX / PPTX / XLSX 提取文字后截断。
//! 统一限制大小，避免把大文件或二进制塞进模型。

use std::fs::File;
use std::io::Read;
use std::path::Path;

use calamine::Reader;

const TEXT_EXTS: &[&str] = &[
    "txt", "md", "markdown", "csv", "tsv", "json", "yaml", "yml", "xml", "html", "htm", "css",
    "js", "mjs", "cjs", "ts", "tsx", "jsx", "py", "rs", "go", "java", "kt", "c", "h", "cpp",
    "hpp", "cs", "sh", "bat", "ps1", "log", "ini", "toml", "sql", "vue", "svelte", "tex",
];

/// 单文件最多读取的原始字节数
const MAX_BYTES: u64 = 256 * 1024;
const MAX_PDF_BYTES: u64 = 20 * 1024 * 1024;
const MAX_ZIP_BYTES: u64 = 50 * 1024 * 1024;
const MAX_ZIP_ENTRY_BYTES: u64 = 5 * 1024 * 1024;
/// 送给模型的摘要字符数上限
pub const MAX_CHARS: usize = 1500;

pub struct Snippet {
    pub text: String,
    pub truncated: bool,
}

pub fn supports(ext: &str) -> bool {
    let ext = ext.to_ascii_lowercase();
    TEXT_EXTS.contains(&ext.as_str())
        || matches!(
            ext.as_str(),
            "pdf" | "docx" | "pptx" | "xlsx" | "xls" | "xlsm" | "xlsb" | "ods"
        )
}

/// 读取内容摘要。返回 None 表示不适合读内容（过大、二进制、空文件）。
pub fn read_snippet(path: &Path, max_chars: usize) -> Result<Option<Snippet>, String> {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let text = match ext.as_str() {
        "pdf" => read_pdf(path)?,
        "docx" => read_zip_text(path, |name| name == "word/document.xml")?,
        "pptx" => read_zip_text(path, |name| {
            name.starts_with("ppt/slides/slide") && name.ends_with(".xml")
        })?,
        "xlsx" | "xls" | "xlsm" | "xlsb" | "ods" => read_spreadsheet(path)?,
        _ if TEXT_EXTS.contains(&ext.as_str()) => read_plain(path)?,
        _ => None,
    };

    let Some(text) = text else {
        return Ok(None);
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

fn file_size(path: &Path) -> Result<u64, String> {
    std::fs::metadata(path)
        .map(|meta| meta.len())
        .map_err(|e| e.to_string())
}

fn read_plain(path: &Path) -> Result<Option<String>, String> {
    let size = file_size(path)?;
    if size == 0 || size > MAX_BYTES {
        return Ok(None);
    }

    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;

    // 含 NUL 的按二进制处理，不读内容
    if bytes.contains(&0) {
        return Ok(None);
    }

    Ok(Some(String::from_utf8_lossy(&bytes).to_string()))
}

fn read_pdf(path: &Path) -> Result<Option<String>, String> {
    let size = file_size(path)?;
    if size == 0 || size > MAX_PDF_BYTES {
        return Ok(None);
    }
    pdf_extract::extract_text(path)
        .map(Some)
        .map_err(|e| format!("PDF 解析失败：{e}"))
}

fn read_spreadsheet(path: &Path) -> Result<Option<String>, String> {
    let size = file_size(path)?;
    if size == 0 || size > MAX_ZIP_BYTES {
        return Ok(None);
    }

    let mut workbook =
        calamine::open_workbook_auto(path).map_err(|e| format!("表格解析失败：{e}"))?;
    let sheets: Vec<String> = workbook
        .sheet_names()
        .iter()
        .map(|name| name.to_string())
        .collect();

    let mut out = String::new();
    for sheet in sheets.iter().take(3) {
        let Ok(range) = workbook.worksheet_range(sheet) else {
            continue;
        };
        for row in range.rows().take(50) {
            let cells: Vec<String> = row
                .iter()
                .take(20)
                .map(|cell| cell.to_string())
                .filter(|value| !value.trim().is_empty())
                .collect();
            if !cells.is_empty() {
                out.push_str(&cells.join(" "));
                out.push('\n');
            }
        }
        if out.chars().count() > MAX_CHARS * 2 {
            break;
        }
    }

    Ok(if out.trim().is_empty() {
        None
    } else {
        Some(out)
    })
}

fn read_zip_text<F>(path: &Path, matches: F) -> Result<Option<String>, String>
where
    F: Fn(&str) -> bool,
{
    let size = file_size(path)?;
    if size == 0 || size > MAX_ZIP_BYTES {
        return Ok(None);
    }

    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("压缩包解析失败：{e}"))?;
    let mut out = String::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        if !matches(entry.name()) || entry.size() > MAX_ZIP_ENTRY_BYTES {
            continue;
        }
        let mut xml = String::new();
        if entry.read_to_string(&mut xml).is_err() {
            continue;
        }
        let text = strip_xml_text(&xml);
        if !text.trim().is_empty() {
            out.push_str(&text);
            out.push(' ');
        }
    }

    Ok(if out.trim().is_empty() {
        None
    } else {
        Some(out)
    })
}

/// 去掉 XML 标签，只留文本节点；同时还原常见实体。
fn strip_xml_text(xml: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in xml.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_entities(&out)
}

fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn extracts_docx_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.docx");
        write_zip(
            &path,
            &[(
                "word/document.xml",
                b"<w:document><w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p><w:p><w:r><w:t>world</w:t></w:r></w:p></w:body></w:document>",
            )],
        );
        let snippet = read_snippet(&path, 100).unwrap().unwrap();
        assert_eq!(snippet.text, "Hello world");
    }

    #[test]
    fn extracts_pptx_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.pptx");
        write_zip(
            &path,
            &[(
                "ppt/slides/slide1.xml",
                b"<p:sld><p:cSld><a:t>Slide</a:t><a:t>title</a:t></p:cSld></p:sld>",
            )],
        );
        let snippet = read_snippet(&path, 100).unwrap().unwrap();
        assert_eq!(snippet.text, "Slide title");
    }

    #[test]
    fn supports_common_formats() {
        assert!(supports("md"));
        assert!(supports("TXT"));
        assert!(supports("pdf"));
        assert!(supports("docx"));
        assert!(supports("pptx"));
        assert!(supports("xlsx"));
        assert!(supports("xls"));
        assert!(supports("ods"));
        assert!(!supports("jpg"));
        assert!(!supports("doc"));
        assert!(!supports("ppt"));
    }
}
