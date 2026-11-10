//! Tauri 命令层。只做参数校验、状态读写和领域逻辑编排，不含业务规则本身。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::State;
use uuid::Uuid;

use crate::domain::planner::build_plan;
use crate::domain::rules::needs_meta;
use crate::error::{AppError, Result};
use crate::infra::execute::{self, RenameTarget};
use crate::infra::meta;
use crate::model::{
    ExecuteRequest, ExecuteResult, FileEntry, MediaMeta, OperationSummary, PresetFile,
    PreviewRequest, PreviewResult, RuleInstance, ScanRequest, ScanResponse, Settings, UndoResult,
};
use crate::state::{AppState, ScanSession};

#[tauri::command]
pub fn scan(state: State<'_, AppState>, req: ScanRequest) -> Result<ScanResponse> {
    let output = crate::infra::scan::scan_dir(&req)?;

    // 「选文件」模式下前端未必传 dir，用第一个条目所在目录兜底，
    // 这样操作日志与历史列表始终有一个可显示的目录
    let session_dir = if req.dir.trim().is_empty() {
        output
            .entries
            .first()
            .map(|e| e.dir.clone())
            .unwrap_or_default()
    } else {
        req.dir.clone()
    };

    let entries = Arc::new(output.entries);
    let session_id = Uuid::new_v4().to_string();

    {
        let mut sessions = state
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // 单窗口应用：只保留最新一次扫描，避免旧会话长期占内存
        sessions.clear();
        sessions.insert(
            session_id.clone(),
            ScanSession {
                dir: session_dir.clone(),
                entries: entries.clone(),
                meta_cache: Arc::new(Mutex::new(HashMap::new())),
            },
        );
    }

    Ok(ScanResponse {
        session_id,
        dir: session_dir,
        entries: (*entries).clone(),
        skipped: output.skipped,
        truncated: output.truncated,
    })
}

/// 清空当前扫描会话。前端「清空列表」时调用，避免上一次的条目长期占着内存。
#[tauri::command]
pub fn clear_session(state: State<'_, AppState>) {
    let mut sessions = state
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    sessions.clear();
}

#[tauri::command]
pub fn preview(state: State<'_, AppState>, req: PreviewRequest) -> Result<PreviewResult> {
    let (_dir, entries) = prepare(&state, &req.session_id, req.ids.as_deref(), &req.rules)?;
    Ok(build_plan(
        &entries,
        None,
        &req.rules,
        &|p| p.exists(),
        req.conflict_policy,
    ))
}

#[tauri::command]
pub fn execute(state: State<'_, AppState>, req: ExecuteRequest) -> Result<ExecuteResult> {
    // 预览与执行走完全相同的「取名 + 填元数据」流程，保证看到的名字就是要落地的名字
    let (dir, entries) = prepare(&state, &req.session_id, req.ids.as_deref(), &req.rules)?;

    let plan = build_plan(
        &entries,
        None,
        &req.rules,
        &|p| p.exists(),
        req.conflict_policy,
    );

    if plan.errors > 0 {
        return Err(AppError::Message(format!(
            "存在 {} 个错误项，请先修正再执行",
            plan.errors
        )));
    }
    if plan.changed == 0 {
        return Err(AppError::Message("没有需要改名的条目".into()));
    }

    let index: HashMap<u64, &FileEntry> = entries.iter().map(|e| (e.id, e)).collect();
    let mut targets = Vec::with_capacity(plan.changed);

    for item in plan.items.iter().filter(|i| i.changed) {
        let entry = index
            .get(&item.id)
            .ok_or_else(|| AppError::Message("条目索引失效，请重新扫描".into()))?;
        let base = PathBuf::from(&entry.dir);
        targets.push(RenameTarget {
            src: base.join(&entry.name),
            dst: base.join(&item.dst_name),
            src_name: entry.name.clone(),
            dst_name: item.dst_name.clone(),
        });
    }

    let rules_snapshot = serde_json::to_value(&req.rules).ok();
    execute::run(&dir, targets, req.conflict_policy, rules_snapshot, &state.history)
}

#[tauri::command]
pub fn undo(state: State<'_, AppState>, operation_id: String) -> Result<UndoResult> {
    state.history.undo(&operation_id)
}

#[tauri::command]
pub fn list_operations(state: State<'_, AppState>) -> Result<Vec<OperationSummary>> {
    state.history.list()
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.config.load_settings()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<()> {
    state.config.save_settings(&settings)
}

#[tauri::command]
pub fn get_presets(state: State<'_, AppState>) -> PresetFile {
    state.config.load_presets()
}

#[tauri::command]
pub fn save_presets(state: State<'_, AppState>, presets: PresetFile) -> Result<()> {
    state.config.save_presets(&presets)
}

/// 取出本次要处理的条目（已按 ids 过滤、必要时填好元数据）。
/// 返回 `(目录, 条目)`，目录用于写操作日志。
fn prepare(
    state: &State<'_, AppState>,
    session_id: &str,
    ids: Option<&[u64]>,
    rules: &[RuleInstance],
) -> Result<(String, Vec<FileEntry>)> {
    let (dir, entries, cache) = {
        let sessions = state
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let s = sessions.get(session_id).ok_or(AppError::SessionMissing)?;
        (s.dir.clone(), s.entries.clone(), s.meta_cache.clone())
    };

    let mut picked: Vec<FileEntry> = match ids {
        Some(list) => {
            let wanted: HashSet<u64> = list.iter().copied().collect();
            entries
                .iter()
                .filter(|e| wanted.contains(&e.id))
                .cloned()
                .collect()
        }
        None => entries.iter().cloned().collect(),
    };

    if needs_meta(rules) {
        fill_meta(&mut picked, &cache);
    }

    Ok((dir, picked))
}

/// 按需读元数据。只读选中条目，读过的进缓存，1 万文件也只付一次代价。
fn fill_meta(items: &mut [FileEntry], cache: &Mutex<HashMap<u64, MediaMeta>>) {
    let mut pending: Vec<usize> = Vec::new();
    {
        let guard = cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for (i, e) in items.iter_mut().enumerate() {
            if e.meta.is_some() {
                continue;
            }
            if let Some(m) = guard.get(&e.id) {
                e.meta = Some(m.clone());
            } else if meta::supports(&e.ext) {
                pending.push(i);
            } else {
                // 类型本来就没有元数据，标记为「已处理」避免每轮重复判断
                e.meta = Some(MediaMeta::default());
            }
        }
    }
    if pending.is_empty() {
        return;
    }

    let jobs: Vec<(usize, PathBuf, String)> = pending
        .iter()
        .map(|&i| {
            let e = &items[i];
            (i, Path::new(&e.dir).join(&e.name), e.ext.clone())
        })
        .collect();

    // 读文件头是纯 IO，可以并行；改名本身仍然严格串行
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8);
    let chunk = jobs.len().div_ceil(workers).max(1);

    let mut results: Vec<(usize, MediaMeta)> = Vec::with_capacity(jobs.len());
    std::thread::scope(|s| {
        let handles: Vec<_> = jobs
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    let mut local: Vec<(usize, MediaMeta)> = Vec::with_capacity(part.len());
                    for (i, path, ext) in part {
                        local.push((*i, meta::read_meta(path, ext)));
                    }
                    local
                })
            })
            .collect();
        for handle in handles {
            if let Ok(part) = handle.join() {
                results.extend(part);
            }
        }
    });

    let mut guard = cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for (i, m) in results {
        let id = items[i].id;
        guard.insert(id, m.clone());
        items[i].meta = Some(m);
    }
}
