//! AI 输出解析与校验。解析失败或结构非法时，错误会回灌给模型修一次。

use serde::Deserialize;

use crate::domain::ai_spec;
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
}
