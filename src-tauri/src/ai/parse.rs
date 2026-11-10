//! AI 输出解析与校验。解析失败或结构非法时，错误会回灌给模型修一次。

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::domain::ai_spec;
use crate::domain::name::is_illegal;
use crate::model::AiSpec;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AiSchemeRaw {
    summary: String,
    spec: AiSpec,
    assumptions: Vec<String>,
    unsupported: Vec<String>,
}

impl Default for AiSchemeRaw {
    fn default() -> Self {
        Self {
            summary: String::new(),
            spec: AiSpec::default(),
            assumptions: Vec::new(),
            unsupported: Vec::new(),
        }
    }
}

pub struct ParsedScheme {
    pub summary: String,
    pub spec: AiSpec,
    pub assumptions: Vec<String>,
    pub unsupported: Vec<String>,
}

pub fn parse_scheme(text: &str) -> Result<ParsedScheme, String> {
    let json = extract_json(text);
    let raw: AiSchemeRaw =
        serde_json::from_str(&json).map_err(|e| format!("返回的 JSON 无法解析：{e}"))?;

    ai_spec::validate(&raw.spec)?;

    let summary = raw.summary.trim().to_string();
    Ok(ParsedScheme {
        summary: if summary.is_empty() {
            "AI 命名方案".into()
        } else {
            summary
        },
        spec: raw.spec,
        assumptions: clean_list(raw.assumptions),
        unsupported: clean_list(raw.unsupported),
    })
}

fn clean_list(items: Vec<String>) -> Vec<String> {
    items
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(8)
        .map(|s| s.chars().take(200).collect())
        .collect()
}

// ---------------------------------------------------------------- 按内容命名

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AiMapRaw {
    summary: String,
    names: Vec<AiMapNameRaw>,
    assumptions: Vec<String>,
    unsupported: Vec<String>,
}

impl Default for AiMapRaw {
    fn default() -> Self {
        Self {
            summary: String::new(),
            names: Vec::new(),
            assumptions: Vec::new(),
            unsupported: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiMapNameRaw {
    id: u64,
    stem: String,
}

pub struct ParsedMap {
    pub summary: String,
    pub names: Vec<(u64, String)>,
    pub assumptions: Vec<String>,
    pub unsupported: Vec<String>,
}

pub fn parse_map_scheme(
    text: &str,
    valid_ids: &HashSet<u64>,
    ext_of: &HashMap<u64, String>,
) -> Result<ParsedMap, String> {
    let json = extract_json(text);
    let raw: AiMapRaw =
        serde_json::from_str(&json).map_err(|e| format!("返回的 JSON 无法解析：{e}"))?;

    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for item in raw.names {
        if !valid_ids.contains(&item.id) || !seen.insert(item.id) {
            continue;
        }
        let mut stem = item.stem.trim().to_string();
        if stem.is_empty() {
            continue;
        }

        // 模型偶尔会把扩展名带回来，按原扩展名剥掉
        if let Some(ext) = ext_of.get(&item.id) {
            if !ext.is_empty() {
                if let Some(pos) = stem.rfind('.') {
                    if stem[pos + 1..].eq_ignore_ascii_case(ext) {
                        stem.truncate(pos);
                    }
                }
            }
        }

        let stem = stem.trim().to_string();
        if stem.is_empty() {
            continue;
        }
        if stem.chars().count() > 80 {
            return Err(format!("名字「{}」过长（最多 80 字）", stem));
        }
        if let Some(c) = stem.chars().find(|c| is_illegal(*c)) {
            return Err(format!("名字「{}」含不允许的字符「{c}」", stem));
        }
        names.push((item.id, stem));
    }

    if names.is_empty() {
        return Err("没有返回任何可用的文件名".into());
    }

    let summary = raw.summary.trim().to_string();
    Ok(ParsedMap {
        summary: if summary.is_empty() {
            "按内容命名".into()
        } else {
            summary
        },
        names,
        assumptions: clean_list(raw.assumptions),
        unsupported: clean_list(raw.unsupported),
    })
}

/// 容忍模型在 JSON 外包了 markdown 代码块或少量解释文字。
fn extract_json(text: &str) -> String {
    let trimmed = text.trim();

    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        let after = after.trim_start();
        let after = after.strip_prefix("json").unwrap_or(after);
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }

    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end > start {
            return trimmed[start..=end].to_string();
        }
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_json() {
        let text = r#"{"summary":"test","spec":{"segments":[{"type":"sequence","start":1,"step":1,"pad":3}],"transforms":[]},"assumptions":[],"unsupported":[]}"#;
        let parsed = parse_scheme(text).unwrap();
        assert_eq!(parsed.summary, "test");
        assert_eq!(parsed.spec.segments.len(), 1);
    }

    #[test]
    fn strips_markdown_fence() {
        let text = "```json\n{\"summary\":\"x\",\"spec\":{\"segments\":[{\"type\":\"literal\",\"text\":\"a\"}]}}\n```";
        let parsed = parse_scheme(text).unwrap();
        assert_eq!(parsed.summary, "x");
    }

    #[test]
    fn rejects_empty_spec() {
        let text = r#"{"summary":"x","spec":{"segments":[],"transforms":[]}}"#;
        assert!(parse_scheme(text).is_err());
    }

    #[test]
    fn parses_map_names_and_strips_ext() {
        let text = r#"{"summary":"x","names":[{"id":1,"stem":"report.txt"},{"id":9,"stem":"ignored"}],"assumptions":[],"unsupported":[]}"#;
        let valid: HashSet<u64> = [1u64, 2].into_iter().collect();
        let ext: HashMap<u64, String> = [(1u64, "txt".to_string())].into_iter().collect();

        let parsed = parse_map_scheme(text, &valid, &ext).unwrap();
        assert_eq!(parsed.names, vec![(1, "report".to_string())]);
    }

    #[test]
    fn map_rejects_illegal_chars() {
        let text = r#"{"summary":"x","names":[{"id":1,"stem":"a:b"}]}"#;
        let valid: HashSet<u64> = [1u64].into_iter().collect();
        let ext = HashMap::new();
        assert!(parse_map_scheme(text, &valid, &ext).is_err());
    }
}
