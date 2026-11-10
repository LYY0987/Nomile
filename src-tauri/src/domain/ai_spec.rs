//! AI 母模板执行器。
//!
//! AI 只负责把自然语言提取成 `AiSpec` 槽位，这里负责把槽位确定性地渲染成文件名。
//! 能力是现有规则的超集：条件分支、正则提取、逐段兜底、多来源时间等。

use chrono::Local;
use regex::Regex;

use crate::domain::name::{normalize_nfc, is_illegal};
use crate::domain::rules::{
    apply_one, format_time, normalize_ext, render_token, NameParts, RuleContext,
};
use crate::model::{
    AiCondition, AiDateSource, AiEmpty, AiExtMode, AiField, AiMetaKey, AiSegment, AiSpec,
    AiTransform, FileEntry, LimitMode, Rule, SliceMode,
};

const MAX_DEPTH: u8 = 4;

/// 执行一份 AI 方案，返回最终的主体名与扩展名。
pub fn apply(ctx: &RuleContext<'_>, spec: &AiSpec) -> Result<NameParts, String> {
    let mut stem = ctx.entry.stem.clone();
    for transform in &spec.transforms {
        stem = apply_one(&to_rule(transform), ctx, &stem)?;
    }

    let rendered = if spec.segments.is_empty() {
        stem.clone()
    } else {
        render_segments(&spec.segments, ctx, &stem, 0)?
    };

    let stem = if rendered.trim().is_empty() {
        match spec.fallback.on_empty {
            AiEmpty::KeepOriginal => ctx.entry.stem.clone(),
            AiEmpty::Empty => String::new(),
        }
    } else {
        rendered
    };

    let ext = match spec.extension.mode {
        AiExtMode::Keep => ctx.entry.ext.clone(),
        AiExtMode::Lower => ctx.entry.ext.to_lowercase(),
        AiExtMode::Upper => ctx.entry.ext.to_uppercase(),
        AiExtMode::Set => normalize_ext(&spec.extension.value),
    };

    Ok(NameParts {
        stem: normalize_nfc(&stem),
        ext: normalize_nfc(&ext),
    })
}

/// 这套方案是否需要读文件头（EXIF / 音视频标签）。
pub fn needs_meta(spec: &AiSpec) -> bool {
    spec.segments.iter().any(segment_needs_meta)
}

fn segment_needs_meta(segment: &AiSegment) -> bool {
    match segment {
        AiSegment::Date {
            source,
            fallback_source,
            ..
        } => *source == AiDateSource::Shot || *fallback_source == Some(AiDateSource::Shot),
        AiSegment::Meta { .. } => true,
        AiSegment::Condition {
            when,
            then,
            otherwise,
        } => {
            matches!(when, AiCondition::MetaPresent { .. })
                || then.iter().any(segment_needs_meta)
                || otherwise.iter().any(segment_needs_meta)
        }
        _ => false,
    }
}

fn to_rule(transform: &AiTransform) -> Rule {
    match transform {
        AiTransform::Replace {
            pattern,
            with,
            regex,
            all,
            case_sensitive,
        } => Rule::Replace {
            find: pattern.clone(),
            replace_with: with.clone(),
            regex: *regex,
            case_sensitive: *case_sensitive,
            all: *all,
        },
        AiTransform::StripPrefix { value } => Rule::StripAffix {
            prefix: value.clone(),
            suffix: String::new(),
        },
        AiTransform::StripSuffix { value } => Rule::StripAffix {
            prefix: String::new(),
            suffix: value.clone(),
        },
        AiTransform::RemoveBrackets {
            round,
            square,
            curly,
            cjk,
            keep_brackets,
        } => Rule::RemoveBrackets {
            round: *round,
            square: *square,
            curly: *curly,
            cjk: *cjk,
            keep_brackets: *keep_brackets,
        },
        AiTransform::StripCopyMarks { trailing_number } => Rule::StripCopyMarks {
            trailing_number: *trailing_number,
        },
        AiTransform::Slice { start, end, keep } => Rule::Slice {
            start: *start,
            end: *end,
            mode: if *keep {
                SliceMode::Keep
            } else {
                SliceMode::Cut
            },
        },
        AiTransform::Case { mode } => Rule::Case { mode: *mode },
        AiTransform::Cleanup {
            spaces,
            strip_illegal,
            trim,
            collapse_repeat,
        } => Rule::Cleanup {
            spaces: *spaces,
            strip_illegal: *strip_illegal,
            trim: *trim,
            collapse_repeat: *collapse_repeat,
        },
        AiTransform::NormalizeSeparators {
            to,
            space,
            underscore,
            hyphen,
            dot,
        } => Rule::NormalizeSeparators {
            to: to.clone(),
            space: *space,
            underscore: *underscore,
            hyphen: *hyphen,
            dot: *dot,
        },
        AiTransform::Limit { max, middle } => Rule::LimitLength {
            max: *max,
            mode: if *middle {
                LimitMode::Middle
            } else {
                LimitMode::Truncate
            },
        },
    }
}

fn render_segments(
    segments: &[AiSegment],
    ctx: &RuleContext<'_>,
    stem: &str,
    depth: u8,
) -> Result<String, String> {
    if depth > MAX_DEPTH {
        return Err("条件分支嵌套过深".into());
    }

    let mut out = String::new();
    for segment in segments {
        match segment {
            AiSegment::Literal { text } => out.push_str(text),
            AiSegment::Field {
                field,
                format,
                fallback,
            } => {
                let token = match field {
                    AiField::Stem => "name".to_string(),
                    AiField::Parent => "parent".to_string(),
                    AiField::Size => "size".to_string(),
                    AiField::Index => {
                        if format.trim().is_empty() {
                            "n".to_string()
                        } else {
                            format!("n:{}", format.trim())
                        }
                    }
                    AiField::Total => "total".to_string(),
                };
                let value = render_token(
                    ctx.entry,
                    stem,
                    &ctx.entry.ext,
                    ctx.index,
                    ctx.total,
                    &token,
                );
                push_value(&mut out, value, fallback);
            }
            AiSegment::Sequence { start, step, pad } => {
                let value = start.saturating_add(ctx.index.saturating_mul(*step));
                if *pad > 0 {
                    out.push_str(&format!("{:0width$}", value, width = *pad as usize));
                } else {
                    out.push_str(&value.to_string());
                }
            }
            AiSegment::Date {
                source,
                format,
                fallback_source,
                fallback,
            } => {
                let mut value = source_ms(*source, ctx)
                    .map(|ms| format_time(ms, format))
                    .unwrap_or_default();
                if value.is_empty() {
                    if let Some(alt) = fallback_source {
                        value = source_ms(*alt, ctx)
                            .map(|ms| format_time(ms, format))
                            .unwrap_or_default();
                    }
                }
                push_value(&mut out, value, fallback);
            }
            AiSegment::Meta {
                key,
                format,
                fallback,
            } => {
                let token = meta_token(*key, format);
                let value = render_token(
                    ctx.entry,
                    stem,
                    &ctx.entry.ext,
                    ctx.index,
                    ctx.total,
                    &token,
                );
                push_value(&mut out, value, fallback);
            }
            AiSegment::Extract {
                pattern,
                group,
                fallback,
            } => {
                let re = Regex::new(pattern).map_err(|e| format!("提取正则无效：{e}"))?;
                let value = re
                    .captures(stem)
                    .and_then(|caps| caps.get(*group as usize))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                push_value(&mut out, value, fallback);
            }
            AiSegment::Condition {
                when,
                then,
                otherwise,
            } => {
                let branch = if eval_condition(when, ctx, stem)? {
                    then
                } else {
                    otherwise
                };
                out.push_str(&render_segments(branch, ctx, stem, depth + 1)?);
            }
            AiSegment::Random { len } => {
                let token = format!("rand:{}", (*len).clamp(1, 32));
                out.push_str(&render_token(
                    ctx.entry,
                    stem,
                    &ctx.entry.ext,
                    ctx.index,
                    ctx.total,
                    &token,
                ));
            }
        }
    }
    Ok(out)
}

fn push_value(out: &mut String, value: String, fallback: &str) {
    if value.is_empty() {
        out.push_str(fallback);
    } else {
        out.push_str(&value);
    }
}

fn meta_token(key: AiMetaKey, format: &str) -> String {
    match key {
        AiMetaKey::Camera => "camera".into(),
        AiMetaKey::Make => "make".into(),
        AiMetaKey::Model => "model".into(),
        AiMetaKey::Lens => "lens".into(),
        AiMetaKey::Title => "title".into(),
        AiMetaKey::Artist => "artist".into(),
        AiMetaKey::Album => "album".into(),
        AiMetaKey::Track => {
            if format.trim().is_empty() {
                "track".into()
            } else {
                format!("track:{}", format.trim())
            }
        }
        AiMetaKey::Duration => {
            if format.trim().is_empty() {
                "duration".into()
            } else {
                format!("duration:{}", format.trim())
            }
        }
        AiMetaKey::Width => "width".into(),
        AiMetaKey::Height => "height".into(),
    }
}

fn source_ms(source: AiDateSource, ctx: &RuleContext<'_>) -> Option<i64> {
    let ms = match source {
        AiDateSource::Modified => ctx.entry.modified,
        AiDateSource::Created => ctx.entry.created,
        AiDateSource::Now => Local::now().timestamp_millis(),
        AiDateSource::Shot => ctx.entry.meta.as_ref().and_then(|m| m.shot).unwrap_or(0),
    };
    (ms > 0).then_some(ms)
}

fn eval_condition(
    condition: &AiCondition,
    ctx: &RuleContext<'_>,
    stem: &str,
) -> Result<bool, String> {
    Ok(match condition {
        AiCondition::ExtIn { values } => values.iter().any(|v| ext_matches(v, &ctx.entry.ext)),
        AiCondition::ExtNotIn { values } => !values.iter().any(|v| ext_matches(v, &ctx.entry.ext)),
        AiCondition::MetaPresent { key } => meta_present(ctx.entry, *key),
        AiCondition::NameMatches { pattern } => {
            let re = Regex::new(pattern).map_err(|e| format!("匹配正则无效：{e}"))?;
            re.is_match(stem)
        }
        AiCondition::IsDir { value } => ctx.entry.is_dir == *value,
        AiCondition::SizeRange { min, max } => {
            min.map_or(true, |m| ctx.entry.size >= m) && max.map_or(true, |m| ctx.entry.size <= m)
        }
    })
}

fn ext_matches(value: &str, ext: &str) -> bool {
    let v = value.trim().trim_start_matches('.').to_lowercase();
    v == "*" || v == ext.to_lowercase()
}

fn meta_present(entry: &FileEntry, key: AiMetaKey) -> bool {
    let Some(meta) = entry.meta.as_ref() else {
        return false;
    };
    match key {
        AiMetaKey::Camera => meta.make.is_some() || meta.model.is_some(),
        AiMetaKey::Make => meta.make.is_some(),
        AiMetaKey::Model => meta.model.is_some(),
        AiMetaKey::Lens => meta.lens.is_some(),
        AiMetaKey::Title => meta.title.is_some(),
        AiMetaKey::Artist => meta.artist.is_some(),
        AiMetaKey::Album => meta.album.is_some(),
        AiMetaKey::Track => meta.track.is_some(),
        AiMetaKey::Duration => meta.duration.is_some(),
        AiMetaKey::Width => meta.width.is_some(),
        AiMetaKey::Height => meta.height.is_some(),
    }
}

/// 结构校验：挡住 AI 生成的可疑方案，错误信息会回灌给模型修一次。
pub fn validate(spec: &AiSpec) -> Result<(), String> {
    if spec.segments.is_empty() && spec.transforms.is_empty() {
        return Err("方案为空：没有生成任何命名段或预处理".into());
    }
    if spec.segments.len() > 24 {
        return Err("命名段过多（最多 24 段）".into());
    }
    if spec.transforms.len() > 24 {
        return Err("预处理过多（最多 24 条）".into());
    }

    let mut count = 0usize;
    for segment in &spec.segments {
        validate_segment(segment, 0, &mut count)?;
    }
    if count > 64 {
        return Err("命名段总数过多（最多 64 段）".into());
    }

    for transform in &spec.transforms {
        match transform {
            AiTransform::Replace { pattern, regex, .. } => {
                if *regex {
                    Regex::new(pattern).map_err(|e| format!("替换正则无效：{e}"))?;
                } else if pattern.chars().count() > 200 {
                    return Err("替换文本过长".into());
                }
            }
            AiTransform::Limit { max, .. } => {
                if !(1..=260).contains(max) {
                    return Err("限制长度的 max 必须在 1..260".into());
                }
            }
            AiTransform::Slice { start, .. } => {
                if start.unsigned_abs() > 260 {
                    return Err("截取位置绝对值不能超过 260".into());
                }
            }
            _ => {}
        }
    }

    if spec.extension.mode == AiExtMode::Set && spec.extension.value.chars().count() > 20 {
        return Err("扩展名设置过长".into());
    }

    Ok(())
}

fn validate_segment(segment: &AiSegment, depth: u8, count: &mut usize) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("条件分支嵌套过深（最多 4 层）".into());
    }
    *count += 1;

    match segment {
        AiSegment::Literal { text } => {
            if text.chars().count() > 120 {
                return Err("固定文字过长（最多 120 字）".into());
            }
            if let Some(c) = text.chars().find(|c| is_illegal(*c)) {
                return Err(format!("固定文字含 Windows 不允许的字符「{c}」"));
            }
        }
        AiSegment::Sequence { step, pad, .. } => {
            if *step == 0 {
                return Err("序号步长不能为 0".into());
            }
            if *pad > 10 {
                return Err("序号补零位数不能超过 10".into());
            }
        }
        AiSegment::Extract { pattern, group, .. } => {
            Regex::new(pattern).map_err(|e| format!("提取正则无效：{e}"))?;
            if *group > 9 {
                return Err("提取的捕获组不能超过 9".into());
            }
        }
        AiSegment::Condition {
            when,
            then,
            otherwise,
        } => {
            if let AiCondition::NameMatches { pattern } = when {
                Regex::new(pattern).map_err(|e| format!("匹配正则无效：{e}"))?;
            }
            for child in then.iter().chain(otherwise.iter()) {
                validate_segment(child, depth + 1, count)?;
            }
        }
        AiSegment::Random { len } => {
            if !(1..=32).contains(len) {
                return Err("随机串长度必须在 1..32".into());
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::name::split_full_name;
    use crate::model::{MediaMeta, RuleInstance, Target};

    fn entry(name: &str) -> FileEntry {
        let (stem, ext) = split_full_name(name, false);
        FileEntry {
            id: 7,
            dir: "C:\\tmp".into(),
            name: name.into(),
            stem,
            ext,
            kind: Default::default(),
            size: 1024,
            modified: 1_700_000_000_000,
            created: 1_700_000_000_000,
            is_dir: false,
            readonly: false,
            meta: None,
        }
    }

    fn ctx<'a>(entry: &'a FileEntry, index: u64) -> RuleContext<'a> {
        RuleContext {
            entry,
            index,
            total: 10,
        }
    }

    fn spec(segments: Vec<AiSegment>, transforms: Vec<AiTransform>) -> AiSpec {
        AiSpec {
            segments,
            transforms,
            ..Default::default()
        }
    }

    #[test]
    fn literal_sequence_and_ext() {
        let e = entry("IMG_001.jpg");
        let s = spec(
            vec![
                AiSegment::Literal {
                    text: "photo_".into(),
                },
                AiSegment::Sequence {
                    start: 1,
                    step: 1,
                    pad: 3,
                },
            ],
            vec![],
        );
        assert_eq!(apply(&ctx(&e, 0), &s).unwrap().full(), "photo_001.jpg");
        assert_eq!(apply(&ctx(&e, 4), &s).unwrap().full(), "photo_005.jpg");
    }

    #[test]
    fn transforms_run_before_segments() {
        let e = entry("IMG_001 (2).png");
        let s = spec(
            vec![AiSegment::Field {
                field: AiField::Stem,
                format: String::new(),
                fallback: String::new(),
            }],
            vec![
                AiTransform::StripPrefix {
                    value: "IMG_".into(),
                },
                AiTransform::RemoveBrackets {
                    round: true,
                    square: true,
                    curly: true,
                    cjk: true,
                    keep_brackets: false,
                },
            ],
        );
        assert_eq!(apply(&ctx(&e, 0), &s).unwrap().full(), "001 .png");
    }

    #[test]
    fn empty_segments_keep_transformed_stem() {
        let e = entry("IMG_001.jpg");
        let s = spec(
            vec![],
            vec![AiTransform::StripPrefix {
                value: "IMG_".into(),
            }],
        );
        assert_eq!(apply(&ctx(&e, 0), &s).unwrap().full(), "001.jpg");
    }

    #[test]
    fn condition_picks_branch() {
        let e = entry("a.jpg");
        let s = spec(
            vec![AiSegment::Condition {
                when: AiCondition::ExtIn {
                    values: vec!["jpg".into(), "png".into()],
                },
                then: vec![AiSegment::Literal {
                    text: "image_".into(),
                }],
                otherwise: vec![AiSegment::Literal {
                    text: "other_".into(),
                }],
            }],
            vec![],
        );
        assert_eq!(apply(&ctx(&e, 0), &s).unwrap().full(), "image_.jpg");
    }

    #[test]
    fn date_falls_back_to_modified() {
        let e = entry("a.jpg");
        let s = spec(
            vec![AiSegment::Date {
                source: AiDateSource::Shot,
                format: "yyyy".into(),
                fallback_source: Some(AiDateSource::Modified),
                fallback: "nodate".into(),
            }],
            vec![],
        );
        let out = apply(&ctx(&e, 0), &s).unwrap();
        assert!(out.stem.starts_with("2023"), "got {}", out.stem);
    }

    #[test]
    fn extract_group() {
        let e = entry("IMG_20230506_123456.jpg");
        let s = spec(
            vec![AiSegment::Extract {
                pattern: r"IMG_(\d{4})(\d{2})(\d{2})".into(),
                group: 1,
                fallback: "x".into(),
            }],
            vec![],
        );
        assert_eq!(apply(&ctx(&e, 0), &s).unwrap().full(), "2023.jpg");
    }

    #[test]
    fn empty_result_keeps_original() {
        let e = entry("a.jpg");
        let s = spec(
            vec![AiSegment::Meta {
                key: AiMetaKey::Title,
                format: String::new(),
                fallback: String::new(),
            }],
            vec![],
        );
        assert_eq!(apply(&ctx(&e, 0), &s).unwrap().full(), "a.jpg");
    }

    #[test]
    fn needs_meta_detects_shot_and_tags() {
        let date = spec(
            vec![AiSegment::Date {
                source: AiDateSource::Shot,
                format: "yyyy".into(),
                fallback_source: None,
                fallback: String::new(),
            }],
            vec![],
        );
        assert!(needs_meta(&date));

        let title = spec(
            vec![AiSegment::Meta {
                key: AiMetaKey::Title,
                format: String::new(),
                fallback: String::new(),
            }],
            vec![],
        );
        assert!(needs_meta(&title));

        let plain = spec(
            vec![AiSegment::Sequence {
                start: 1,
                step: 1,
                pad: 3,
            }],
            vec![],
        );
        assert!(!needs_meta(&plain));
    }

    #[test]
    fn media_meta_is_used() {
        let mut e = entry("a.mp3");
        e.meta = Some(MediaMeta {
            title: Some("song".into()),
            artist: Some("band".into()),
            ..Default::default()
        });
        let s = spec(
            vec![
                AiSegment::Meta {
                    key: AiMetaKey::Artist,
                    format: String::new(),
                    fallback: String::new(),
                },
                AiSegment::Literal { text: "-".into() },
                AiSegment::Meta {
                    key: AiMetaKey::Title,
                    format: String::new(),
                    fallback: String::new(),
                },
            ],
            vec![],
        );
        assert_eq!(apply(&ctx(&e, 0), &s).unwrap().full(), "band-song.mp3");
    }

    #[test]
    fn invalid_regex_is_rejected() {
        let e = entry("a.jpg");
        let s = spec(
            vec![AiSegment::Extract {
                pattern: "(".into(),
                group: 1,
                fallback: String::new(),
            }],
            vec![],
        );
        assert!(apply(&ctx(&e, 0), &s).is_err());
        assert!(validate(&s).is_err());
    }

    #[test]
    fn rule_instance_dispatches_ai_spec() {
        let e = entry("IMG_001.jpg");
        let inst = RuleInstance {
            id: "ai".into(),
            enabled: true,
            target: Target::Name,
            rule: Rule::Ai {
                spec: spec(
                    vec![AiSegment::Sequence {
                        start: 1,
                        step: 1,
                        pad: 2,
                    }],
                    vec![AiTransform::StripPrefix {
                        value: "IMG_".into(),
                    }],
                ),
            },
        };
        let out = crate::domain::rules::apply_stack(&ctx(&e, 0), &[inst]).unwrap();
        assert_eq!(out.full(), "01.jpg");
    }
}
