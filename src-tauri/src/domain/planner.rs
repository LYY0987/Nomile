//! 计划构建与校验。
//!
//! `build_plan` 本身是纯函数，唯一的外部依赖是传入的 `path_exists` 闭包——
//! 这样单测可以完全离线，同时也让「预览」和「执行」共用同一套判定逻辑。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::domain::name;
use crate::domain::rules::{apply_stack, needs_meta, RuleContext};
use crate::model::{
    ConflictPolicy, FileEntry, Issue, IssueKind, IssueLevel, PlanItem, PreviewResult, RuleInstance,
};

pub fn build_plan<F: Fn(&Path) -> bool>(
    entries: &[FileEntry],
    ids: Option<&[u64]>,
    rules: &[RuleInstance],
    path_exists: &F,
    policy: ConflictPolicy,
) -> PreviewResult {
    let selected: Vec<&FileEntry> = match ids {
        Some(list) => {
            let wanted: HashSet<u64> = list.iter().copied().collect();
            entries.iter().filter(|e| wanted.contains(&e.id)).collect()
        }
        None => entries.iter().collect(),
    };

    let total = selected.len();
    // 规则里用到了元数据变量时才提示「文件没有元数据」，否则纯改名场景不该冒黄条
    let meta_needed = needs_meta(rules);
    let mut items: Vec<PlanItem> = Vec::with_capacity(total);
    let mut name_src: HashMap<u64, String> = HashMap::with_capacity(total);

    for (index, entry) in selected.iter().enumerate() {
        let ctx = RuleContext {
            entry,
            index: index as u64,
            total: total as u64,
        };

        let mut issues: Vec<Issue> = Vec::new();
        let dst_name = match apply_stack(&ctx, rules) {
            Ok(parts) => parts.full(),
            Err(msg) => {
                issues.push(Issue {
                    level: IssueLevel::Error,
                    kind: IssueKind::EmptyName,
                    message: msg,
                });
                entry.name.clone()
            }
        };

        let (diff_start, diff_end) = diff_span(&entry.name, &dst_name);

        name_src.insert(entry.id, entry.name.clone());
        items.push(PlanItem {
            id: entry.id,
            src_name: entry.name.clone(),
            dst_name,
            changed: false, // 稍后统一计算
            diff_start,
            diff_end,
            issues,
        });
    }

    // ---- 逐条合法性检查
    let mut changed_flags: Vec<bool> = Vec::with_capacity(total);
    for (i, item) in items.iter_mut().enumerate() {
        let entry = selected[i];
        item.changed = item.dst_name != item.src_name;
        changed_flags.push(item.changed);

        if !item.changed {
            continue;
        }

        let trimmed = item
            .dst_name
            .trim_matches(|c: char| c.is_whitespace() || c == '.');
        if trimmed.is_empty() {
            item.issues.push(issue(
                IssueLevel::Error,
                IssueKind::EmptyName,
                "新文件名为空".into(),
            ));
        }

        if let Some(c) = name::first_illegal(&item.dst_name) {
            item.issues.push(issue(
                IssueLevel::Error,
                IssueKind::IllegalChar,
                format!("新文件名含 Windows 不允许的字符「{c}」"),
            ));
        }

        if !entry.is_dir {
            let (stem, _) = name::split_full_name(&item.dst_name, false);
            if name::is_reserved(&stem) {
                item.issues.push(issue(
                    IssueLevel::Error,
                    IssueKind::ReservedName,
                    "新文件名是 Windows 保留设备名".into(),
                ));
            }
        }

        let full_len = name::utf16_len(&entry.dir) + 1 + name::utf16_len(&item.dst_name);
        if full_len > name::MAX_PATH {
            item.issues.push(issue(
                IssueLevel::Warning,
                IssueKind::TooLong,
                format!("完整路径将达 {full_len} 字符，可能超出 Windows 限制（{MAX}）", MAX = name::MAX_PATH),
            ));
        }

        if item
            .src_name
            .to_lowercase()
            .eq(&item.dst_name.to_lowercase())
        {
            item.issues.push(issue(
                IssueLevel::Info,
                IssueKind::CaseOnly,
                "仅改变大小写，执行时会走临时名两步完成".into(),
            ));
        }

        if meta_needed && !entry.meta.as_ref().map(|m| m.has_value()).unwrap_or(false) {
            item.issues.push(issue(
                IssueLevel::Warning,
                IssueKind::MetaMissing,
                "这个文件里没有可用的拍摄时间 / 标签，元数据变量会留空".into(),
            ));
        }
    }

    // ---- 批次内重名
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        if !changed_flags[i] {
            continue;
        }
        let key = format!(
            "{}|{}",
            selected[i].dir.to_lowercase(),
            item.dst_name.to_lowercase()
        );
        groups.entry(key).or_default().push(i);
    }
    for idxs in groups.values().filter(|v| v.len() > 1) {
        let names: Vec<String> = idxs.iter().map(|i| items[*i].dst_name.clone()).collect();
        let repr = names[0].clone();
        for i in idxs {
            items[*i].issues.push(issue(
                IssueLevel::Error,
                IssueKind::Duplicate,
                format!("与其他 {} 个条目产生同名「{repr}」", names.len() - 1),
            ));
        }
    }

    // ---- 目标已存在于磁盘（批次内将被释放的名字不算）
    let released: HashSet<String> = items
        .iter()
        .enumerate()
        .filter(|(i, _)| changed_flags[*i])
        .map(|(i, item)| {
            format!(
                "{}|{}",
                selected[i].dir.to_lowercase(),
                item.src_name.to_lowercase()
            )
        })
        .collect();

    for (i, item) in items.iter_mut().enumerate() {
        if !changed_flags[i] {
            continue;
        }
        let target = PathBuf::from(&selected[i].dir).join(&item.dst_name);
        let key = format!(
            "{}|{}",
            selected[i].dir.to_lowercase(),
            item.dst_name.to_lowercase()
        );
        if released.contains(&key) {
            continue;
        }
        if path_exists(&target) {
            let (level, message) = match policy {
                ConflictPolicy::AutoSuffix => (
                    IssueLevel::Warning,
                    "目标文件已存在，将自动加 (1)".to_string(),
                ),
                ConflictPolicy::Skip => (
                    IssueLevel::Info,
                    "目标文件已存在，该文件会被跳过".to_string(),
                ),
            };
            item.issues.push(issue(level, IssueKind::ExistsOnDisk, message));
        }
    }

    // ---- 汇总
    let changed = changed_flags.iter().filter(|c| **c).count();
    let errors = items
        .iter()
        .flat_map(|i| i.issues.iter())
        .filter(|i| i.level == IssueLevel::Error)
        .count();
    let warnings = items
        .iter()
        .flat_map(|i| i.issues.iter())
        .filter(|i| i.level == IssueLevel::Warning)
        .count();

    PreviewResult {
        items,
        total,
        changed,
        errors,
        warnings,
        can_execute: errors == 0 && changed > 0,
    }
}

fn issue(level: IssueLevel, kind: IssueKind, message: String) -> Issue {
    Issue {
        level,
        kind,
        message,
    }
}

/// 返回目标名中与源名不同的字符区间 `[start, end)`，按字符索引。
/// 用「公共前后缀」近似差异区间：对文件名这种短字符串足够直观，
/// 且是 O(n)，不需要上 LCS。
fn diff_span(a: &str, b: &str) -> (usize, usize) {
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();

    let max_prefix = ac.len().min(bc.len());
    let mut prefix = 0usize;
    while prefix < max_prefix && ac[prefix] == bc[prefix] {
        prefix += 1;
    }

    let mut suffix = 0usize;
    while suffix < max_prefix - prefix
        && ac[ac.len() - 1 - suffix] == bc[bc.len() - 1 - suffix]
    {
        suffix += 1;
    }

    (prefix, bc.len() - suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConflictPolicy, Rule, Target};

    fn entry(id: u64, name: &str) -> FileEntry {
        let (stem, ext) = name::split_full_name(name, false);
        FileEntry {
            id,
            dir: "C:\\tmp".into(),
            name: name.into(),
            stem,
            ext,
            size: 1,
            modified: 0,
            created: 0,
            is_dir: false,
            readonly: false,
            meta: None,
        }
    }

    fn inst(id: &str, rule: Rule) -> RuleInstance {
        RuleInstance {
            id: id.into(),
            enabled: true,
            target: Target::Name,
            rule,
        }
    }

    fn no_file(_: &Path) -> bool {
        false
    }

    #[test]
    fn empty_rules_change_nothing() {
        let entries = vec![entry(1, "a.txt"), entry(2, "b.txt")];
        let plan = build_plan(&entries, None, &[], &no_file, ConflictPolicy::AutoSuffix);
        assert_eq!(plan.total, 2);
        assert_eq!(plan.changed, 0);
        assert!(!plan.can_execute);
        assert!(plan.items.iter().all(|i| !i.changed));
    }

    #[test]
    fn affix_marks_changed() {
        let entries = vec![entry(1, "a.txt"), entry(2, "b.txt")];
        let rules = vec![inst(
            "r1",
            Rule::Affix {
                prefix: "n_".into(),
                suffix: String::new(),
            },
        )];
        let plan = build_plan(&entries, None, &rules, &no_file, ConflictPolicy::AutoSuffix);
        assert_eq!(plan.changed, 2);
        assert!(plan.can_execute);
        assert_eq!(plan.items[0].dst_name, "n_a.txt");
    }

    #[test]
    fn duplicate_targets_are_errors() {
        let entries = vec![entry(1, "a.txt"), entry(2, "b.txt")];
        let rules = vec![inst(
            "r1",
            Rule::Template {
                pattern: "same.${ext}".into(),
            },
        )];
        let plan = build_plan(&entries, None, &rules, &no_file, ConflictPolicy::AutoSuffix);
        assert_eq!(plan.errors, 2);
        assert!(!plan.can_execute);
        assert!(plan.items[0]
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::Duplicate));
    }

    #[test]
    fn illegal_char_is_error() {
        let entries = vec![entry(1, "a.txt")];
        let rules = vec![inst(
            "r1",
            Rule::Replace {
                find: "a".into(),
                replace_with: "a:b".into(),
                regex: false,
                case_sensitive: true,
                all: true,
            },
        )];
        let plan = build_plan(&entries, None, &rules, &no_file, ConflictPolicy::AutoSuffix);
        assert_eq!(plan.errors, 1);
        assert!(plan.items[0]
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::IllegalChar));
    }

    #[test]
    fn reserved_name_is_error() {
        let entries = vec![entry(1, "a.txt")];
        let rules = vec![inst(
            "r1",
            Rule::Template {
                pattern: "CON.${ext}".into(),
            },
        )];
        let plan = build_plan(&entries, None, &rules, &no_file, ConflictPolicy::AutoSuffix);
        assert!(plan.items[0]
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::ReservedName));
    }

    #[test]
    fn existing_target_is_warning() {
        let entries = vec![entry(1, "a.txt")];
        let rules = vec![inst(
            "r1",
            Rule::Affix {
                prefix: "x".into(),
                suffix: String::new(),
            },
        )];
        let exists = |p: &Path| p.file_name().map(|n| n == "xa.txt").unwrap_or(false);
        let plan = build_plan(&entries, None, &rules, &exists, ConflictPolicy::AutoSuffix);
        assert_eq!(plan.warnings, 1);
        assert!(plan.can_execute);
    }

    #[test]
    fn meta_missing_is_warned() {
        // 规则用了 ${shot}，但这个文件没有元数据 → 应该有一条 warning 提示，而不是静默生成空名字
        let entries = vec![entry(1, "a.jpg")];
        let rules = vec![inst(
            "r1",
            Rule::Template {
                pattern: "${shot}_${orig}.${ext}".into(),
            },
        )];
        let plan = build_plan(&entries, None, &rules, &no_file, ConflictPolicy::AutoSuffix);
        assert!(
            plan.items[0]
                .issues
                .iter()
                .any(|i| i.kind == IssueKind::MetaMissing),
            "缺元数据应给出提示"
        );
    }

    #[test]
    fn chain_rename_not_flagged_as_existing() {
        // 1.jpg→2.jpg 且 2.jpg→3.jpg：2.jpg 既是目标又是源，本批次会让位，不应报「已存在」
        let entries = vec![entry(1, "1.jpg"), entry(2, "2.jpg")];
        let rules = vec![inst(
            "r1",
            Rule::Sequence {
                start: 2,
                step: 1,
                pad: 0,
                position: crate::model::SeqPosition::Replace,
                separator: String::new(),
            },
        )];
        let exists = |p: &Path| {
            p.file_name()
                .map(|n| n == "1.jpg" || n == "2.jpg")
                .unwrap_or(false)
        };
        let plan = build_plan(&entries, None, &rules, &exists, ConflictPolicy::AutoSuffix);
        assert!(
            plan.items
                .iter()
                .all(|i| !i.issues.iter().any(|x| x.kind == IssueKind::ExistsOnDisk)),
            "批次内互相让位的名字不应报已存在"
        );
        assert_eq!(plan.items[0].dst_name, "2.jpg");
        assert_eq!(plan.items[1].dst_name, "3.jpg");
    }

    #[test]
    fn case_only_is_info() {
        let entries = vec![entry(1, "abc.txt")];
        let rules = vec![inst(
            "r1",
            Rule::Case {
                mode: crate::model::CaseKind::Upper,
            },
        )];
        let plan = build_plan(&entries, None, &rules, &no_file, ConflictPolicy::AutoSuffix);
        assert_eq!(plan.items[0].dst_name, "ABC.txt");
        assert!(plan.items[0]
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::CaseOnly));
        assert!(plan.can_execute);
    }

    #[test]
    fn selection_subset_only() {
        let entries = vec![entry(1, "a.txt"), entry(2, "b.txt"), entry(3, "c.txt")];
        let rules = vec![inst(
            "r1",
            Rule::Affix {
                prefix: "x".into(),
                suffix: String::new(),
            },
        )];
        let plan = build_plan(&entries, Some(&[1, 3]), &rules, &no_file, ConflictPolicy::AutoSuffix);
        assert_eq!(plan.total, 2);
        assert_eq!(plan.items[0].id, 1);
        assert_eq!(plan.items[1].id, 3);
    }

    #[test]
    fn invalid_regex_surfaces_as_issue() {
        let entries = vec![entry(1, "a.txt")];
        let rules = vec![inst(
            "r1",
            Rule::Replace {
                find: "(".into(),
                replace_with: String::new(),
                regex: true,
                case_sensitive: true,
                all: true,
            },
        )];
        let plan = build_plan(&entries, None, &rules, &no_file, ConflictPolicy::AutoSuffix);
        assert_eq!(plan.items[0].dst_name, "a.txt");
        assert!(plan.items[0].issues.iter().any(|i| i.level == IssueLevel::Error));
    }

    #[test]
    fn diff_span_points_at_changed_part() {
        assert_eq!(diff_span("a.txt", "x_a.txt"), (0, 2));
        let (s, e) = diff_span("IMG_1234.jpg", "IMG_1234_edit.jpg");
        assert_eq!(s, 8);
        assert_eq!(e, 13);
        assert_eq!(diff_span("same", "same"), (4, 4));
    }
}
