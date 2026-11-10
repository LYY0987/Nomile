//! 规则引擎。全部是纯函数：给定「当前名字 + 上下文」，返回新名字，不碰磁盘。
//!
//! 设计要点：
//! - 规则按栈顺序串联，前一条的输出是后一条的输入；
//! - 每条规则只作用于「文件名」或「扩展名」其中之一（由 `Target` 决定），
//!   要同时改两边就放两条规则，语义清晰、可解释；
//! - `Template` 是例外：它作用于「完整文件名」，必须自己写 `${ext}`。

use std::path::Path;

use chrono::{Local, TimeZone};
use regex::{Regex, RegexBuilder};

use crate::domain::name::{self, normalize_nfc, split_full_name};
use crate::model::{
    CaseKind, FileEntry, MediaMeta, Rule, RuleInstance, SeqPosition, SpaceMode, Target,
};

#[derive(Debug, Clone, Default)]
pub struct NameParts {
    pub stem: String,
    pub ext: String,
}

impl NameParts {
    pub fn full(&self) -> String {
        name::compose(&self.stem, &self.ext)
    }
}

pub struct RuleContext<'a> {
    pub entry: &'a FileEntry,
    /// 在本次预览集合中的序号，0 起
    pub index: u64,
    pub total: u64,
}

/// 模板里能用到的、必须打开文件才拿得到的变量
pub const META_VARS: &[&str] = &[
    "shot", "camera", "make", "model", "lens", "title", "artist", "album", "track", "duration",
    "width", "height",
];

/// 这套规则里有没有用到元数据变量——决定要不要去读文件头。
/// 读 EXIF / 标签要打开上万次文件，纯改名场景不该付这个代价。
pub fn needs_meta(rules: &[RuleInstance]) -> bool {
    rules
        .iter()
        .filter(|r| r.enabled)
        .any(|r| matches!(&r.rule, Rule::Template { pattern } if pattern_uses_meta(pattern)))
}

fn pattern_uses_meta(pattern: &str) -> bool {
    let mut rest = pattern;
    while let Some(pos) = rest.find("${") {
        let after = &rest[pos + 2..];
        let Some(end) = after.find('}') else { break };
        let token = &after[..end];
        let key = token.split(':').next().unwrap_or("").trim().to_lowercase();
        if META_VARS.contains(&key.as_str()) {
            return true;
        }
        rest = &after[end..];
    }
    false
}

/// 依次应用全部启用的规则，返回最终名字。
/// 出错（例如正则写错）时返回人话错误，由上层转成 Error 级 issue。
pub fn apply_stack(ctx: &RuleContext<'_>, rules: &[RuleInstance]) -> Result<NameParts, String> {
    let mut parts = NameParts {
        stem: ctx.entry.stem.clone(),
        ext: ctx.entry.ext.clone(),
    };

    for inst in rules.iter().filter(|r| r.enabled) {
        if let Rule::Template { pattern } = &inst.rule {
            let rendered = render_template(
                pattern,
                &RenderCtx {
                    entry: ctx.entry,
                    stem: &parts.stem,
                    ext: &parts.ext,
                    index: ctx.index,
                    total: ctx.total,
                },
            );
            let normalized = normalize_nfc(&rendered);
            let (s, e) = split_full_name(&normalized, ctx.entry.is_dir);
            parts.stem = s;
            parts.ext = e;
            continue;
        }

        let current = match inst.target {
            Target::Name => parts.stem.clone(),
            Target::Extension => parts.ext.clone(),
        };
        let updated = apply_one(&inst.rule, ctx, &current)?;
        match inst.target {
            Target::Name => parts.stem = updated,
            Target::Extension => parts.ext = updated,
        }
    }

    parts.stem = normalize_nfc(&parts.stem);
    parts.ext = normalize_nfc(&parts.ext);
    Ok(parts)
}

fn apply_one(rule: &Rule, ctx: &RuleContext<'_>, input: &str) -> Result<String, String> {
    Ok(match rule {
        Rule::Replace {
            find,
            replace_with,
            regex,
            case_sensitive,
            all,
        } => apply_replace(find, replace_with, *regex, *case_sensitive, *all, input)?,
        Rule::Sequence {
            start,
            step,
            pad,
            position,
            separator,
        } => apply_sequence(*start, *step, *pad, *position, separator, ctx.index, input),
        Rule::Case { mode } => apply_case(*mode, input),
        Rule::Affix { prefix, suffix } => format!("{prefix}{input}{suffix}"),
        Rule::Cleanup {
            spaces,
            strip_illegal,
            trim,
            collapse_repeat,
        } => apply_cleanup(*spaces, *strip_illegal, *trim, *collapse_repeat, input),
        // Template 在 apply_stack 里单独处理
        Rule::Template { .. } => input.to_string(),
    })
}

// ---------------------------------------------------------------- 各规则实现

fn build_regex(find: &str, is_regex: bool, case_sensitive: bool) -> Result<Regex, String> {
    // 字面替换也走正则路径：`regex::escape` 保证等价，
    // 同时避免手写的大小写不敏感替换在 Unicode 折叠时切错字节。
    let body = if is_regex {
        find.to_string()
    } else {
        regex::escape(find)
    };
    RegexBuilder::new(&body)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| format!("正则无效：{e}"))
}

fn apply_replace(
    find: &str,
    replace_with: &str,
    is_regex: bool,
    case_sensitive: bool,
    all: bool,
    input: &str,
) -> Result<String, String> {
    if find.is_empty() {
        return Ok(input.to_string());
    }
    let re = build_regex(find, is_regex, case_sensitive)?;
    if all {
        Ok(re.replace_all(input, replace_with).into_owned())
    } else {
        Ok(re.replace(input, replace_with).into_owned())
    }
}

fn apply_sequence(
    start: u64,
    step: u64,
    pad: u8,
    position: SeqPosition,
    separator: &str,
    index: u64,
    input: &str,
) -> String {
    let value = start.saturating_add(index.saturating_mul(step));
    let num = format!("{:0width$}", value, width = pad as usize);
    match position {
        SeqPosition::Prefix => format!("{num}{separator}{input}"),
        SeqPosition::Suffix => format!("{input}{separator}{num}"),
        SeqPosition::Replace => num,
    }
}

fn apply_case(mode: CaseKind, input: &str) -> String {
    match mode {
        CaseKind::Upper => input.to_uppercase(),
        CaseKind::Lower => input.to_lowercase(),
        CaseKind::Title => to_title(input),
        CaseKind::Sentence => to_sentence(input),
    }
}

fn to_title(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut at_word_start = true;
    for c in input.chars() {
        if c.is_alphanumeric() {
            if at_word_start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            at_word_start = false;
        } else {
            out.push(c);
            at_word_start = true;
        }
    }
    out
}

fn to_sentence(input: &str) -> String {
    let lowered = input.to_lowercase();
    let mut chars = lowered.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => lowered,
    }
}

fn apply_cleanup(
    spaces: SpaceMode,
    strip_illegal: bool,
    trim: bool,
    collapse_repeat: bool,
    input: &str,
) -> String {
    let mut s = input.to_string();

    if strip_illegal {
        s = s.chars().filter(|c| !name::is_illegal(*c)).collect();
    }

    s = match spaces {
        SpaceMode::Keep => s,
        SpaceMode::Underscore => s.replace(' ', "_"),
        SpaceMode::Remove => s.replace(' ', ""),
        SpaceMode::Collapse => collapse_whitespace(&s),
    };

    if collapse_repeat {
        s = dedup_chars(&s);
    }

    if trim {
        s = s.trim_matches(|c: char| c.is_whitespace() || c == '.').to_string();
    }

    s
}

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_space = false;
    for c in input.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                out.push(' ');
            }
            last_was_space = true;
        } else {
            out.push(c);
            last_was_space = false;
        }
    }
    out
}

fn dedup_chars(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev: Option<char> = None;
    for c in input.chars() {
        if prev != Some(c) {
            out.push(c);
            prev = Some(c);
        }
    }
    out
}

// ---------------------------------------------------------------- 模板

struct RenderCtx<'a> {
    entry: &'a FileEntry,
    stem: &'a str,
    ext: &'a str,
    index: u64,
    total: u64,
}

/// `$$` 转义成 `$`；`${...}` 按变量求值；无法识别的变量原样保留，便于用户看出写错。
fn render_template(pattern: &str, ctx: &RenderCtx<'_>) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len());
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() {
            if chars[i + 1] == '$' {
                out.push('$');
                i += 2;
                continue;
            }
            if chars[i + 1] == '{' {
                if let Some(offset) = chars[i + 2..].iter().position(|c| *c == '}') {
                    let token: String = chars[i + 2..i + 2 + offset].iter().collect();
                    out.push_str(&eval_token(&token, ctx));
                    i = i + 2 + offset + 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}

fn eval_token(token: &str, ctx: &RenderCtx<'_>) -> String {
    let (var, arg) = match token.find(':') {
        Some(p) => (&token[..p], &token[p + 1..]),
        None => (token, ""),
    };
    let key = var.trim().to_lowercase();
    let meta = ctx.entry.meta.as_ref();

    match key.as_str() {
        "name" => ctx.stem.to_string(),
        "orig" => ctx.entry.stem.clone(),
        "full" => ctx.entry.name.clone(),
        "ext" => ctx.ext.to_string(),
        "n" | "i" | "index" => {
            let width = arg.trim().parse::<usize>().unwrap_or(0);
            format!("{:0width$}", ctx.index + 1, width = width)
        }
        "total" => ctx.total.to_string(),
        "parent" => Path::new(&ctx.entry.dir)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        "size" => ctx.entry.size.to_string(),
        "date" | "mtime" => format_time(ctx.entry.modified, arg),
        "ctime" => format_time(ctx.entry.created, arg),
        "rand" => {
            let len = arg.trim().parse::<usize>().unwrap_or(6).clamp(1, 32);
            // 用条目 id 做种子，保证预览与执行得到同一个值
            deterministic_alnum(ctx.entry.id, len)
        }
        // ---- 元数据（读文件头才有，缺失时留空，由校验器提示）
        "shot" => shot_text(meta, arg),
        "camera" => camera_text(meta),
        "make" | "model" | "lens" | "title" | "artist" | "album" => meta_text(meta, key.as_str()),
        "track" => track_text(meta, arg),
        "duration" => duration_text(meta, arg),
        "width" => meta_num(meta.and_then(|m| m.width)),
        "height" => meta_num(meta.and_then(|m| m.height)),
        _ => format!("${{{token}}}"),
    }
}

/// 拍摄时间默认 `yyyy-MM-dd_HHmmss`：同一天连拍几十张时只有带时分秒才不会重名
fn shot_text(meta: Option<&MediaMeta>, fmt: &str) -> String {
    let Some(ms) = meta.and_then(|m| m.shot) else {
        return String::new();
    };
    let pattern = if fmt.trim().is_empty() {
        "yyyy-MM-dd_HHmmss"
    } else {
        fmt.trim()
    };
    format_time(ms, pattern)
}

/// 厂商 + 型号；型号里已经含厂商名时不重复写（如 make=NIKON / model=NIKON D750）
fn camera_text(meta: Option<&MediaMeta>) -> String {
    let Some(m) = meta else {
        return String::new();
    };
    let make = m.make.as_deref().unwrap_or("").trim();
    let model = m.model.as_deref().unwrap_or("").trim();
    if model.is_empty() {
        make.to_string()
    } else if make.is_empty() || model.to_lowercase().contains(&make.to_lowercase()) {
        model.to_string()
    } else {
        format!("{make}_{model}")
    }
}

fn meta_text(meta: Option<&MediaMeta>, key: &str) -> String {
    let Some(m) = meta else {
        return String::new();
    };
    let v = match key {
        "make" => m.make.as_deref(),
        "model" => m.model.as_deref(),
        "lens" => m.lens.as_deref(),
        "title" => m.title.as_deref(),
        "artist" => m.artist.as_deref(),
        "album" => m.album.as_deref(),
        _ => None,
    };
    v.unwrap_or("").trim().to_string()
}

/// `${track}` 补零到 2 位，`${track:3}` 可指定宽度
fn track_text(meta: Option<&MediaMeta>, arg: &str) -> String {
    let Some(t) = meta.and_then(|m| m.track) else {
        return String::new();
    };
    let w = arg.trim().parse::<usize>().unwrap_or(2).clamp(1, 6);
    format!("{:0width$}", t, width = w)
}

/// `${duration}` 输出整秒，`${duration:mmss}` 输出 `02m35s`
fn duration_text(meta: Option<&MediaMeta>, arg: &str) -> String {
    let Some(secs) = meta.and_then(|m| m.duration) else {
        return String::new();
    };
    let total = secs.round() as u64;
    if arg.trim().eq_ignore_ascii_case("mmss") {
        format!("{:02}m{:02}s", total / 60, total % 60)
    } else {
        total.to_string()
    }
}

fn meta_num(v: Option<u32>) -> String {
    v.map(|n| n.to_string()).unwrap_or_default()
}

fn to_chrono_fmt(fmt: &str) -> String {
    fmt.replace("yyyy", "%Y")
        .replace("MM", "%m")
        .replace("dd", "%d")
        .replace("HH", "%H")
        .replace("mm", "%M")
        .replace("ss", "%S")
}

fn format_time(ms: i64, fmt: &str) -> String {
    let pattern = if fmt.trim().is_empty() {
        "yyyy-MM-dd"
    } else {
        fmt.trim()
    };
    let formatter = to_chrono_fmt(pattern);
    match Local.timestamp_millis_opt(ms).single() {
        Some(dt) => dt.format(&formatter).to_string(),
        None => String::new(),
    }
    .replace(':', "-")
    .replace(' ', "_")
}

fn deterministic_alnum(seed: u64, len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut state = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    if state == 0 {
        state = 0x9E37_79B9_7F4A_7C15;
    }
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.push(ALPHABET[(state as usize) % ALPHABET.len()] as char);
    }
    out
}

// ---------------------------------------------------------------- 测试

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CaseKind, Rule, SeqPosition, SpaceMode, Target};

    fn entry(name: &str, stem: &str, ext: &str, modified: i64) -> FileEntry {
        FileEntry {
            id: 7,
            dir: "C:\\tmp".into(),
            name: name.into(),
            stem: stem.into(),
            ext: ext.into(),
            size: 1024,
            modified,
            created: modified,
            is_dir: false,
            readonly: false,
            meta: None,
        }
    }

    fn inst(target: Target, rule: Rule) -> RuleInstance {
        RuleInstance {
            id: "r1".into(),
            enabled: true,
            target,
            rule,
        }
    }

    fn run(entry: &FileEntry, rules: &[RuleInstance], index: u64) -> String {
        let ctx = RuleContext {
            entry,
            index,
            total: 10,
        };
        apply_stack(&ctx, rules).unwrap().full()
    }

    #[test]
    fn literal_replace_all() {
        let e = entry("a-b-c.txt", "a-b-c", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Replace {
                find: "-".into(),
                replace_with: "_".into(),
                regex: false,
                case_sensitive: true,
                all: true,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "a_b_c.txt");
    }

    #[test]
    fn literal_replace_first_only() {
        let e = entry("a-b-c.txt", "a-b-c", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Replace {
                find: "-".into(),
                replace_with: "_".into(),
                regex: false,
                case_sensitive: true,
                all: false,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "a_b-c.txt");
    }

    #[test]
    fn case_insensitive_replace() {
        let e = entry("Photo.JPG", "Photo", "JPG", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Replace {
                find: "photo".into(),
                replace_with: "pic".into(),
                regex: false,
                case_sensitive: false,
                all: true,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "pic.JPG");
    }

    #[test]
    fn regex_with_capture_group() {
        let e = entry("IMG_20230101_123456.jpg", "IMG_20230101_123456", "jpg", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Replace {
                find: r"IMG_(\d{4})(\d{2})(\d{2})_(\d+)".into(),
                replace_with: "${1}-${2}-${3}_${4}".into(),
                regex: true,
                case_sensitive: true,
                all: true,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "2023-01-01_123456.jpg");
    }

    #[test]
    fn invalid_regex_reports_error() {
        let e = entry("a.txt", "a", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Replace {
                find: "(".into(),
                replace_with: "x".into(),
                regex: true,
                case_sensitive: true,
                all: true,
            },
        )];
        let ctx = RuleContext {
            entry: &e,
            index: 0,
            total: 1,
        };
        assert!(apply_stack(&ctx, &rules).is_err());
    }

    #[test]
    fn sequence_prefix_padded() {
        let e = entry("a.txt", "a", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Sequence {
                start: 1,
                step: 1,
                pad: 3,
                position: SeqPosition::Prefix,
                separator: "_".into(),
            },
        )];
        assert_eq!(run(&e, &rules, 0), "001_a.txt");
        assert_eq!(run(&e, &rules, 4), "005_a.txt");
    }

    #[test]
    fn sequence_suffix_no_separator() {
        let e = entry("a.txt", "a", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Sequence {
                start: 10,
                step: 5,
                pad: 0,
                position: SeqPosition::Suffix,
                separator: "-".into(),
            },
        )];
        assert_eq!(run(&e, &rules, 2), "a-20.txt");
    }

    #[test]
    fn sequence_replace_whole_name() {
        let e = entry("a.txt", "a", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Sequence {
                start: 1,
                step: 1,
                pad: 2,
                position: SeqPosition::Replace,
                separator: String::new(),
            },
        )];
        assert_eq!(run(&e, &rules, 5), "06.txt");
    }

    #[test]
    fn case_title() {
        let e = entry("hello world.txt", "hello world", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Case {
                mode: CaseKind::Title,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "Hello World.txt");
    }

    #[test]
    fn case_sentence() {
        let e = entry("HELLO WORLD.TXT", "HELLO WORLD", "TXT", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Case {
                mode: CaseKind::Sentence,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "Hello world.TXT");
    }

    #[test]
    fn extension_case_lower() {
        let e = entry("a.JPG", "a", "JPG", 0);
        let rules = vec![inst(
            Target::Extension,
            Rule::Case {
                mode: CaseKind::Lower,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "a.jpg");
    }

    #[test]
    fn affix_both_sides() {
        let e = entry("a.txt", "a", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Affix {
                prefix: "2023_".into(),
                suffix: "_final".into(),
            },
        )];
        assert_eq!(run(&e, &rules, 0), "2023_a_final.txt");
    }

    #[test]
    fn cleanup_spaces_and_illegal() {
        let e = entry("a  b:c.txt", "a  b:c", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Cleanup {
                spaces: SpaceMode::Underscore,
                strip_illegal: true,
                trim: true,
                collapse_repeat: false,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "a__bc.txt");
    }

    #[test]
    fn cleanup_collapse_and_dedup() {
        let e = entry("aaa   file.txt", "aaa   file", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Cleanup {
                spaces: SpaceMode::Collapse,
                strip_illegal: false,
                trim: true,
                collapse_repeat: true,
            },
        )];
        // dedup 的副作用：a 压成一个，空格也压成一个，但空格后紧跟的 a 与前面紧邻的 a 合并
        assert_eq!(run(&e, &rules, 0), "a file.txt");
    }

    #[test]
    fn template_variables() {
        let e = entry("a.txt", "a", "txt", 1_700_000_000_000);
        let rules = vec![inst(
            Target::Name,
            Rule::Template {
                pattern: "${name}_${n:3}.${ext}".into(),
            },
        )];
        assert_eq!(run(&e, &rules, 4), "a_005.txt");
    }

    #[test]
    fn template_orig_keeps_original_stem() {
        let e = entry("a.txt", "a", "txt", 0);
        let rules = vec![
            inst(
                Target::Name,
                Rule::Affix {
                    prefix: "x_".into(),
                    suffix: String::new(),
                },
            ),
            inst(
                Target::Name,
                Rule::Template {
                    pattern: "${orig}_${name}.${ext}".into(),
                },
            ),
        ];
        assert_eq!(run(&e, &rules, 0), "a_x_a.txt");
    }

    #[test]
    fn template_dollar_escape() {
        let e = entry("a.txt", "a", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Template {
                pattern: "$$${name}.${ext}".into(),
            },
        )];
        assert_eq!(run(&e, &rules, 0), "$a.txt");
    }

    #[test]
    fn template_unknown_variable_kept() {
        let e = entry("a.txt", "a", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Template {
                pattern: "${wrong}.${ext}".into(),
            },
        )];
        assert_eq!(run(&e, &rules, 0), "${wrong}.txt");
    }

    #[test]
    fn template_rand_is_deterministic() {
        let e = entry("a.txt", "a", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Template {
                pattern: "${rand:6}${rand:6}.${ext}".into(),
            },
        )];
        let a = run(&e, &rules, 0);
        let b = run(&e, &rules, 0);
        assert_eq!(a, b);
        assert_eq!(a.len(), 6 + 6 + 4);
    }

    fn entry_with_meta(name: &str, stem: &str, ext: &str, meta: MediaMeta) -> FileEntry {
        let mut e = entry(name, stem, ext, 0);
        e.meta = Some(meta);
        e
    }

    #[test]
    fn needs_meta_detects_meta_vars() {
        let plain = Rule::Template {
            pattern: "${name}_${n}.${ext}".into(),
        };
        assert!(!needs_meta(&[inst(Target::Name, plain)]));

        let with_meta = Rule::Template {
            pattern: "${shot}_${name}.${ext}".into(),
        };
        assert!(needs_meta(&[inst(Target::Name, with_meta)]));

        // 关掉的规则不该触发读文件
        let mut disabled = inst(
            Target::Name,
            Rule::Template {
                pattern: "${shot}.${ext}".into(),
            },
        );
        disabled.enabled = false;
        assert!(!needs_meta(&[disabled]));
    }

    #[test]
    fn camera_joins_or_dedups() {
        let mut m = MediaMeta::default();
        m.make = Some("Canon".into());
        m.model = Some("EOS R5".into());
        assert_eq!(camera_text(Some(&m)), "Canon_EOS R5");

        let mut m2 = MediaMeta::default();
        m2.make = Some("NIKON".into());
        m2.model = Some("NIKON D750".into());
        assert_eq!(camera_text(Some(&m2)), "NIKON D750");

        let mut m3 = MediaMeta::default();
        m3.model = Some("D750".into());
        assert_eq!(camera_text(Some(&m3)), "D750");
        assert_eq!(camera_text(None), "");
    }

    #[test]
    fn shot_missing_leaves_empty() {
        assert_eq!(shot_text(None, ""), "");
        assert_eq!(shot_text(Some(&MediaMeta::default()), ""), "");
    }

    #[test]
    fn duration_formats() {
        let m = MediaMeta {
            duration: Some(155.4),
            ..Default::default()
        };
        assert_eq!(duration_text(Some(&m), ""), "155");
        assert_eq!(duration_text(Some(&m), "mmss"), "02m35s");
        assert_eq!(duration_text(None, ""), "");
    }

    #[test]
    fn track_is_padded() {
        let m = MediaMeta {
            track: Some(3),
            ..Default::default()
        };
        assert_eq!(track_text(Some(&m), ""), "03");
        assert_eq!(track_text(Some(&m), "3"), "003");
    }

    #[test]
    fn template_uses_meta_variables() {
        let ts = 1_672_531_200_000i64;
        let expected = Local
            .timestamp_millis_opt(ts)
            .single()
            .unwrap()
            .format("%Y-%m-%d_%H%M%S")
            .to_string();

        let meta = MediaMeta {
            shot: Some(ts),
            title: Some("  ".into()), // 空白标题应被当成空
            ..Default::default()
        };
        let e = entry_with_meta("a.jpg", "a", "jpg", meta);
        let rules = vec![inst(
            Target::Name,
            Rule::Template {
                pattern: "${shot}_${title}.${ext}".into(),
            },
        )];
        assert_eq!(run(&e, &rules, 0), format!("{expected}_.jpg"));
    }

    #[test]
    fn chained_rules_order_matters() {
        let e = entry("a-b.txt", "a-b", "txt", 0);
        let rules = vec![
            inst(
                Target::Name,
                Rule::Replace {
                    find: "-".into(),
                    replace_with: "_".into(),
                    regex: false,
                    case_sensitive: true,
                    all: true,
                },
            ),
            inst(
                Target::Name,
                Rule::Case {
                    mode: CaseKind::Upper,
                },
            ),
        ];
        assert_eq!(run(&e, &rules, 0), "A_B.txt");
    }

    #[test]
    fn disabled_rule_is_skipped() {
        let e = entry("a.txt", "a", "txt", 0);
        let mut r = inst(
            Target::Name,
            Rule::Affix {
                prefix: "x_".into(),
                suffix: String::new(),
            },
        );
        r.enabled = false;
        assert_eq!(run(&e, &[r], 0), "a.txt");
    }
}
