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
    CaseKind, FileEntry, LimitMode, MediaMeta, Rule, RuleInstance, SeqPosition, SliceMode,
    SpaceMode, Target, TimeSource,
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

        if let Rule::SetExtension { ext } = &inst.rule {
            // 这条规则天然只改扩展名，不看 target
            parts.ext = normalize_ext(ext);
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
        Rule::Slice { start, end, mode } => apply_slice(*start, *end, *mode, input),
        Rule::Insert { at, text } => apply_insert(*at, text, input),
        Rule::RemoveBrackets {
            round,
            square,
            curly,
            cjk,
            keep_brackets,
        } => apply_remove_brackets(*round, *square, *curly, *cjk, *keep_brackets, input),
        Rule::StripCopyMarks { trailing_number } => apply_strip_copy(*trailing_number, input),
        Rule::LimitLength { max, mode } => apply_limit(*max, *mode, input),
        Rule::NormalizeSeparators {
            to,
            space,
            underscore,
            hyphen,
            dot,
        } => apply_normalize_sep(to, *space, *underscore, *hyphen, *dot, input),
        Rule::StripAffix { prefix, suffix } => apply_strip_affix(prefix, suffix, input),
        Rule::InsertTime {
            source,
            format,
            position,
            separator,
        } => apply_insert_time(*source, format, *position, separator, ctx, input),
        // Template 在 apply_stack 里单独处理
        Rule::Template { .. } => input.to_string(),
        // SetExtension 在 apply_stack 里单独处理
        Rule::SetExtension { .. } => input.to_string(),
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

// ---------------------------------------------------------------- 位置类规则

/// 下标统一 1 基：正数从前往后，负数从末尾数，`end=0` 表示一直到末尾。
/// 越界一律夹到边界，空区间就是空串，不会 panic。
fn apply_slice(start: i64, end: i64, mode: SliceMode, input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len() as i64;
    if len == 0 {
        return String::new();
    }

    let begin = if start > 0 {
        (start - 1).min(len)
    } else if start < 0 {
        (len + start).max(0)
    } else {
        0
    };

    let finish = if end == 0 {
        len
    } else if end > 0 {
        end.min(len)
    } else {
        (len + end + 1).clamp(0, len)
    };

    let (b, f) = (begin.clamp(0, len) as usize, finish.clamp(0, len) as usize);

    match mode {
        SliceMode::Keep => {
            if b < f {
                chars[b..f].iter().collect()
            } else {
                String::new()
            }
        }
        SliceMode::Cut => {
            let head: String = chars[..b].iter().collect();
            let tail: String = chars[f..].iter().collect();
            format!("{head}{tail}")
        }
    }
}

fn apply_insert(at: i64, text: &str, input: &str) -> String {
    if text.is_empty() {
        return input.to_string();
    }
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len() as i64;
    let idx = if at > 0 {
        (at - 1).min(len)
    } else if at < 0 {
        (len + at + 1).clamp(0, len)
    } else {
        0
    } as usize;

    let mut out: String = chars[..idx].iter().collect();
    out.push_str(text);
    out.extend(chars[idx..].iter());
    out
}

/// 括号成对消掉；找不到配对闭括号的开括号原样保留，避免把整串吃掉
fn apply_remove_brackets(
    round: bool,
    square: bool,
    curly: bool,
    cjk: bool,
    keep_brackets: bool,
    input: &str,
) -> String {
    let chars: Vec<char> = input.chars().collect();
    let pairs: Vec<(char, char)> = [
        ('(', ')', round),
        ('[', ']', square),
        ('{', '}', curly),
        ('（', '）', cjk),
        ('【', '】', cjk),
    ]
    .into_iter()
    .filter(|(_, _, on)| *on)
    .map(|(o, c, _)| (o, c))
    .collect();

    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(&(_, close)) = pairs.iter().find(|(open, _)| *open == c) {
            if let Some(rel) = chars[i + 1..].iter().position(|x| *x == close) {
                if keep_brackets {
                    out.push(c);
                    out.push(close);
                }
                i += rel + 2;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn apply_strip_copy(trailing_number: bool, input: &str) -> String {
    let mut s = input.trim_end().to_string();
    // 逐个剥：`xxx - 副本 (2)` 先掉 `(2)`，下一轮再掉 ` - 副本`
    for _ in 0..8 {
        // 拷成独立 String 再判断，避免一边借用 s 一边重新赋值
        let trimmed = s.trim_end().to_string();
        let mut hit = false;

        if trailing_number {
            if let Some(rest) = strip_trailing_number(&trimmed) {
                s = rest;
                hit = true;
            }
        }
        if !hit {
            if let Some(rest) = strip_trailing_copy_word(&trimmed) {
                s = rest;
                hit = true;
            }
        }
        if !hit {
            break;
        }
    }
    s.trim_end().to_string()
}

/// 只认 1-3 位纯数字，`报告 (2024)` 这种年份不会被误删
fn strip_trailing_number(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    let close = *chars.last()?;
    let open = match close {
        ')' => '(',
        '）' => '（',
        _ => return None,
    };
    let pos = chars.iter().rposition(|c| *c == open)?;
    let inner: String = chars[pos + 1..chars.len() - 1].iter().collect();
    if inner.is_empty() || inner.len() > 3 || !inner.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(chars[..pos].iter().collect::<String>().trim_end().to_string())
}

fn strip_trailing_copy_word(s: &str) -> Option<String> {
    let lower = s.to_lowercase();
    for suffix in [" - 副本", "- 副本", " - copy", "- copy", "副本", " copy"] {
        if lower.ends_with(suffix) {
            // 用字节长度回切：suffix 都是完整字符序列，切点必然落在 UTF-8 边界
            return Some(s[..s.len() - suffix.len()].trim_end().to_string());
        }
    }
    None
}

fn apply_limit(max: usize, mode: LimitMode, input: &str) -> String {
    let max = max.max(1);
    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= max {
        return input.to_string();
    }
    match mode {
        LimitMode::Truncate => chars[..max].iter().collect(),
        LimitMode::Middle => {
            if max == 1 {
                return "…".to_string();
            }
            let keep = max - 1;
            let head_n = (keep + 1) / 2;
            let tail_n = keep - head_n;
            let head: String = chars[..head_n].iter().collect();
            let tail: String = chars[chars.len() - tail_n..].iter().collect();
            format!("{head}…{tail}")
        }
    }
}

fn apply_normalize_sep(
    to: &str,
    space: bool,
    underscore: bool,
    hyphen: bool,
    dot: bool,
    input: &str,
) -> String {
    let target = if to.is_empty() { "_" } else { to };
    let is_source = |c: char| -> bool {
        (space && c.is_whitespace())
            || (underscore && c == '_')
            || (hyphen && matches!(c, '-' | '\u{2010}' | '\u{2011}' | '\u{FF0D}'))
            || (dot && matches!(c, '.' | '\u{00B7}'))
    };

    let mut out = String::with_capacity(input.len());
    let mut pending = false;
    for c in input.chars() {
        if is_source(c) {
            pending = true;
            continue;
        }
        if pending {
            // 开头的分隔符直接丢掉，别让名字以「_」起步
            if !out.is_empty() {
                out.push_str(target);
            }
            pending = false;
        }
        out.push(c);
    }
    out
}

fn apply_strip_affix(prefix: &str, suffix: &str, input: &str) -> String {
    let mut s: &str = input;
    if !prefix.is_empty() {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
        }
    }
    if !suffix.is_empty() {
        if let Some(rest) = s.strip_suffix(suffix) {
            s = rest;
        }
    }
    s.to_string()
}

fn apply_insert_time(
    source: TimeSource,
    format: &str,
    position: SeqPosition,
    separator: &str,
    ctx: &RuleContext<'_>,
    input: &str,
) -> String {
    let ms = match source {
        TimeSource::Modified => ctx.entry.modified,
        TimeSource::Created => ctx.entry.created,
        TimeSource::Now => Local::now().timestamp_millis(),
    };
    let stamp = format_time(ms, format);
    if stamp.is_empty() {
        return input.to_string();
    }
    match position {
        SeqPosition::Prefix => format!("{stamp}{separator}{input}"),
        SeqPosition::Suffix => format!("{input}{separator}{stamp}"),
        SeqPosition::Replace => stamp,
    }
}

fn normalize_ext(ext: &str) -> String {
    ext.trim().trim_start_matches('.').trim().to_string()
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

    // ---- 位置类规则

    #[test]
    fn slice_keep_range() {
        let e = entry("IMG_20230101_120000.jpg", "IMG_20230101_120000", "jpg", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Slice {
                start: 5,
                end: 12,
                mode: SliceMode::Keep,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "20230101.jpg");
    }

    #[test]
    fn slice_negative_indexes() {
        let e = entry("abcdef.txt", "abcdef", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Slice {
                start: -3,
                end: -1,
                mode: SliceMode::Keep,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "def.txt");
    }

    #[test]
    fn slice_cut_removes_prefix() {
        let e = entry("IMG_2023_photo.jpg", "IMG_2023_photo", "jpg", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Slice {
                start: 1,
                end: 4,
                mode: SliceMode::Cut,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "2023_photo.jpg");
    }

    #[test]
    fn slice_out_of_range_never_panics() {
        let e = entry("ab.txt", "ab", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Slice {
                start: 99,
                end: 200,
                mode: SliceMode::Keep,
            },
        )];
        assert_eq!(run(&e, &rules, 0), ".txt");
    }

    #[test]
    fn insert_at_position() {
        let e = entry("abcdef.txt", "abcdef", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Insert {
                at: 4,
                text: "XX".into(),
            },
        )];
        assert_eq!(run(&e, &rules, 0), "abcXXdef.txt");
    }

    #[test]
    fn insert_negative_goes_to_end() {
        let e = entry("abcdef.txt", "abcdef", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::Insert {
                at: -1,
                text: "XX".into(),
            },
        )];
        assert_eq!(run(&e, &rules, 0), "abcdefXX.txt");
    }

    #[test]
    fn remove_brackets_both_styles() {
        let e = entry("【站点】电影(1080p).mp4", "【站点】电影(1080p)", "mp4", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::RemoveBrackets {
                round: true,
                square: true,
                curly: true,
                cjk: true,
                keep_brackets: false,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "电影.mp4");
    }

    #[test]
    fn remove_brackets_keeps_brackets_when_asked() {
        let e = entry("电影[1080p].mp4", "电影[1080p]", "mp4", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::RemoveBrackets {
                round: false,
                square: true,
                curly: false,
                cjk: false,
                keep_brackets: true,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "电影[].mp4");
    }

    #[test]
    fn strip_copy_marks() {
        for (src, want) in [
            ("报告 - 副本", "报告"),
            ("报告 - 副本 (3)", "报告"),
            ("照片(2)", "照片"),
            ("照片（3）", "照片"),
            ("file - Copy", "file"),
            ("报告 (2024)", "报告 (2024)"), // 年份不该被当成副本编号
        ] {
            let e = entry(&format!("{src}.txt"), src, "txt", 0);
            let rules = vec![inst(
                Target::Name,
                Rule::StripCopyMarks {
                    trailing_number: true,
                },
            )];
            assert_eq!(run(&e, &rules, 0), format!("{want}.txt"), "输入 {src}");
        }
    }

    #[test]
    fn limit_length_modes() {
        let e = entry("abcdefghij.txt", "abcdefghij", "txt", 0);
        let trunc = vec![inst(
            Target::Name,
            Rule::LimitLength {
                max: 5,
                mode: LimitMode::Truncate,
            },
        )];
        assert_eq!(run(&e, &trunc, 0), "abcde.txt");

        let middle = vec![inst(
            Target::Name,
            Rule::LimitLength {
                max: 5,
                mode: LimitMode::Middle,
            },
        )];
        assert_eq!(run(&e, &middle, 0), "ab…ij.txt");
    }

    #[test]
    fn set_extension_ignores_target() {
        let e = entry("a.JPG", "a", "JPG", 0);
        let rules = vec![inst(
            // 目标选的是文件名，这条规则照样只改扩展名
            Target::Name,
            Rule::SetExtension { ext: ".jpg".into() },
        )];
        assert_eq!(run(&e, &rules, 0), "a.jpg");
    }

    #[test]
    fn set_extension_empty_removes_suffix() {
        let e = entry("a.txt", "a", "txt", 0);
        let rules = vec![inst(
            Target::Extension,
            Rule::SetExtension { ext: String::new() },
        )];
        assert_eq!(run(&e, &rules, 0), "a");
    }

    #[test]
    fn normalize_separators_unifies() {
        let e = entry("a b_c-d.e.txt", "a b_c-d.e", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::NormalizeSeparators {
                to: "_".into(),
                space: true,
                underscore: true,
                hyphen: true,
                dot: true,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "a_b_c_d_e.txt");
    }

    #[test]
    fn normalize_separators_drops_leading_sep() {
        let e = entry("_ _a.txt", "_ _a", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::NormalizeSeparators {
                to: "_".into(),
                space: true,
                underscore: true,
                hyphen: true,
                dot: true,
            },
        )];
        assert_eq!(run(&e, &rules, 0), "a.txt");
    }

    #[test]
    fn strip_affix_only_once() {
        let e = entry("[站] 歌_v2.txt", "[站] 歌_v2", "txt", 0);
        let rules = vec![inst(
            Target::Name,
            Rule::StripAffix {
                prefix: "[站] ".into(),
                suffix: "_v2".into(),
            },
        )];
        assert_eq!(run(&e, &rules, 0), "歌.txt");
    }

    #[test]
    fn insert_time_prefix() {
        let ts = 1_700_000_000_000i64;
        let expected = Local
            .timestamp_millis_opt(ts)
            .single()
            .unwrap()
            .format("%Y%m%d")
            .to_string();
        let e = entry("a.txt", "a", "txt", ts);
        let rules = vec![inst(
            Target::Name,
            Rule::InsertTime {
                source: TimeSource::Modified,
                format: "yyyyMMdd".into(),
                position: SeqPosition::Prefix,
                separator: "_".into(),
            },
        )];
        assert_eq!(run(&e, &rules, 0), format!("{expected}_a.txt"));
    }
}
