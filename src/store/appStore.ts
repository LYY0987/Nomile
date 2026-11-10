import { create } from "zustand";

import { api, errorText } from "../lib/ipc";
import type {
  ConflictPolicy,
  FileEntry,
  OperationSummary,
  PlanItem,
  PreviewResult,
  Preset,
  Rule,
  RuleInstance,
  RuleKind,
  ScanRequest,
  SortBy,
  Target,
} from "../lib/types";
import { applyTheme, loadTheme, saveTheme, type Theme } from "../lib/theme";

let seq = 0;
/** 扫描选项变化后的重扫防抖句柄（输入过滤词时不至于每个字符扫一遍） */
let rescanTimer: number | null = null;
function newRuleId(): string {
  seq += 1;
  return `r${Date.now().toString(36)}-${seq}`;
}

function createRule(kind: RuleKind): Rule {
  switch (kind) {
    case "replace":
      return {
        kind: "replace",
        find: "",
        replaceWith: "",
        regex: false,
        caseSensitive: true,
        all: true,
      };
    case "sequence":
      return {
        kind: "sequence",
        start: 1,
        step: 1,
        pad: 3,
        position: "suffix",
        separator: "_",
      };
    case "template":
      return { kind: "template", pattern: "${name}_${n:3}.${ext}" };
    case "case":
      return { kind: "case", mode: "lower" };
    case "affix":
      return { kind: "affix", prefix: "", suffix: "" };
    case "cleanup":
      return {
        kind: "cleanup",
        spaces: "collapse",
        stripIllegal: true,
        trim: true,
        collapseRepeat: false,
      };
    case "slice":
      return { kind: "slice", start: 1, end: 0, mode: "keep" };
    case "insert":
      return { kind: "insert", at: 1, text: "" };
    case "removeBrackets":
      return {
        kind: "removeBrackets",
        round: true,
        square: true,
        curly: true,
        cjk: true,
        keepBrackets: false,
      };
    case "stripCopyMarks":
      return { kind: "stripCopyMarks", trailingNumber: true };
    case "limitLength":
      return { kind: "limitLength", max: 60, mode: "truncate" };
    case "setExtension":
      return { kind: "setExtension", ext: "" };
    case "normalizeSeparators":
      return {
        kind: "normalizeSeparators",
        to: "_",
        space: true,
        underscore: true,
        hyphen: true,
        dot: true,
      };
    case "stripAffix":
      return { kind: "stripAffix", prefix: "", suffix: "" };
    case "insertTime":
      return {
        kind: "insertTime",
        source: "modified",
        format: "yyyyMMdd",
        position: "prefix",
        separator: "_",
      };
  }
}

export function makeRule(kind: RuleKind, target: Target = "name"): RuleInstance {
  return { id: newRuleId(), enabled: true, target, rule: createRule(kind) };
}

/** Windows 路径拼接 / 取父目录，够用就行，不引 path 库 */
function joinPath(dir: string, name: string): string {
  return dir.endsWith("\\") || dir.endsWith("/") ? dir + name : `${dir}\\${name}`;
}

function parentOf(path: string): string | null {
  const i = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return i > 0 ? path.slice(0, i) : null;
}

/** 重扫后恢复选中：原本全选就继续全选，否则按「目录 + 文件名」把还在的条目找回来 */
function nextSelection(
  next: FileEntry[],
  prev: FileEntry[],
  prevSelected: number[],
  wasAll: boolean,
  keep: boolean
): number[] {
  if (!keep || prev.length === 0 || wasAll) return next.map((e) => e.id);
  const sel = new Set(prevSelected);
  const keys = new Set(
    prev.filter((e) => sel.has(e.id)).map((e) => `${e.dir}|${e.name}`)
  );
  return next.filter((e) => keys.has(`${e.dir}|${e.name}`)).map((e) => e.id);
}

export interface ScanOptions {
  recursive: boolean;
  includeHidden: boolean;
  includeDirs: boolean;
  includePattern: string;
  excludePattern: string;
  sortBy: SortBy;
  sortDesc: boolean;
}

/** 来源模式：处理一个目录，还是处理亲手点选的一批文件 */
export type SourceMode = "dir" | "files";

interface Toast {
  /** 每次弹出递增，Toast 组件靠它重置倒计时 */
  id: number;
  text: string;
  kind: "ok" | "err" | "info";
  /** 操作按钮，给了才渲染，比如「撤销」 */
  actionLabel?: string;
  action?: () => void;
  /** 秒；给了才显示倒计时并按这个时长关闭 */
  seconds?: number;
}

let toastSeq = 0;

interface AppState {
  dir: string;
  mode: SourceMode;
  files: string[];
  sessionId: string | null;
  entries: FileEntry[];
  selected: number[];
  /** 列表内搜索词，只影响显示与「全选/反选」的范围 */
  query: string;
  /** 最近一次执行在文件模式下造成的路径变化，撤销时用来把清单改回去 */
  renamedFiles: { from: string; to: string }[];
  rules: RuleInstance[];
  plan: PreviewResult | null;
  policy: ConflictPolicy;
  options: ScanOptions;

  scanning: boolean;
  executing: boolean;
  toast: Toast | null;
  showToast: (t: Omit<Toast, "id">) => void;

  historyOpen: boolean;
  history: OperationSummary[];
  presets: Preset[];
  recent: string[];

  /** 界面主题：只影响外观，与数据无关 */
  theme: Theme;
  toggleTheme: () => void;

  setDir: (dir: string) => void;
  /** 切换来源：目录模式或文件模式（选文件会把 dir 置空，改由 files 决定范围） */
  setSource: (patch: { mode?: SourceMode; dir?: string; files?: string[] }) => void;
  setOptions: (patch: Partial<ScanOptions>) => void;
  /** 改「载入条件」并自动重扫：范围变了，列表就该重新载入 */
  applyOptions: (patch: Partial<ScanOptions>) => void;
  setQuery: (query: string) => void;
  scan: (dir?: string, keepSelection?: boolean) => Promise<void>;
  /** 扫描条件变了之后调用：防抖后自动重扫，保持当前选中 */
  rescanSoon: () => void;
  /** 清空前端的列表（不碰磁盘），回到「选一个目录开始」 */
  clearList: () => void;

  addRule: (kind: RuleKind) => void;
  patchRule: (id: string, patch: Record<string, unknown>) => void;
  patchInstance: (id: string, patch: Partial<RuleInstance>) => void;
  removeRule: (id: string) => void;
  moveRule: (id: string, delta: number) => void;
  clearRules: () => void;
  setRules: (rules: RuleInstance[]) => void;

  toggleSelect: (id: number) => void;
  /** ids 为空时作用于整个列表；传了候选集合就只在该集合内操作 */
  selectAll: (ids?: number[]) => void;
  selectNone: () => void;
  selectInvert: (ids?: number[]) => void;

  setPolicy: (policy: ConflictPolicy) => void;
  refresh: () => Promise<void>;
  run: () => Promise<void>;
  undoOperation: (operationId: string) => Promise<void>;
  undoLast: () => Promise<void>;

  toggleHistory: (open?: boolean) => Promise<void>;
  loadPresets: () => Promise<void>;
  savePreset: (name: string) => Promise<void>;
  applyPreset: (id: string) => void;
  deletePreset: (id: string) => Promise<void>;

  notify: (text: string, kind?: Toast["kind"]) => void;
  clearToast: () => void;
}

export const useApp = create<AppState>((set, get) => ({
  dir: "",
  mode: "dir",
  files: [],
  sessionId: null,
  entries: [],
  selected: [],
  query: "",
  renamedFiles: [],
  rules: [],
  plan: null,
  policy: "autoSuffix",
  options: {
    recursive: false,
    includeHidden: false,
    includeDirs: false,
    includePattern: "",
    excludePattern: "",
    sortBy: "name",
    sortDesc: false,
  },

  scanning: false,
  executing: false,
  toast: null,

  historyOpen: false,
  history: [],
  presets: [],
  recent: [],

  theme: loadTheme(),
  toggleTheme: () => {
    const next: Theme = get().theme === "dark" ? "light" : "dark";
    applyTheme(next);
    saveTheme(next);
    set({ theme: next });
  },

  setDir: (dir) => set({ dir }),
  setOptions: (patch) => set((s) => ({ options: { ...s.options, ...patch } })),
  setSource: (patch) => set(patch),
  setQuery: (query) => set({ query }),

  applyOptions: (patch) => {
    set((s) => ({ options: { ...s.options, ...patch } }));
    // 只在已经载入列表时重扫；用户主动清空后就别自动把列表扫回来
    if (get().sessionId) get().rescanSoon();
  },

  scan: async (dirOverride, keepSelection = false) => {
    const mode = get().mode;
    const picked = get().files;
    let dir = (dirOverride ?? get().dir).trim();

    if (mode === "dir" && !dir) {
      get().notify("请先选择要整理的目录", "err");
      return;
    }
    if (mode === "files" && picked.length === 0) {
      get().notify("请先选择要处理的文件", "err");
      return;
    }
    if (mode === "files" && !dir) {
      // 文件模式下 dir 只用来记录来源，取第一个文件的父目录
      dir = parentOf(picked[0]) ?? "";
    }

    const options = get().options;
    const req: ScanRequest = {
      dir,
      files: mode === "files" ? picked : null,
      recursive: options.recursive,
      includeFiles: true,
      includeDirs: options.includeDirs,
      includeHidden: options.includeHidden,
      includePattern: options.includePattern,
      excludePattern: options.excludePattern,
      sortBy: options.sortBy,
      sortDesc: options.sortDesc,
    };

    // 重扫前记下当前选中：仅因为改了扫描条件而重扫时，用户勾掉的那些应该还勾着
    const prevEntries = get().entries;
    const prevSelected = get().selected;
    const wasAllSelected =
      prevEntries.length > 0 && prevSelected.length === prevEntries.length;

    set({ scanning: true, plan: null });
    try {
      const res = await api.scan(req);
      set({
        dir: res.dir,
        sessionId: res.sessionId,
        entries: res.entries,
        selected: nextSelection(
          res.entries,
          prevEntries,
          prevSelected,
          wasAllSelected,
          keepSelection
        ),
      });
      if (res.truncated) {
        get().notify("条目数量过多，已截断显示", "info");
      } else if (res.skipped > 0) {
        get().notify(`已跳过 ${res.skipped} 个无法读取的条目`, "info");
      }

      // 记住最近用过的目录，下次打开直接回到这里
      const o = get().options;
      void api
        .saveSettings({
          recursive: o.recursive,
          includeHidden: o.includeHidden,
          includeFiles: true,
          includeDirs: o.includeDirs,
          sortBy: o.sortBy,
          sortDesc: o.sortDesc,
          conflictPolicy: get().policy,
          confirmBeforeExecute: false,
          // 不再记录最近目录：下次打开一律从空白开始
          recentDirs: [],
        })
        .catch(() => {});

      await get().refresh();
    } catch (e) {
      get().notify(errorText(e), "err");
    } finally {
      set({ scanning: false });
    }
  },

  addRule: (kind) =>
    set((s) => ({ rules: [...s.rules, makeRule(kind)] })),

  patchRule: (id, patch) =>
    set((s) => ({
      rules: s.rules.map((r) =>
        r.id === id ? { ...r, rule: { ...r.rule, ...patch } as Rule } : r
      ),
    })),

  patchInstance: (id, patch) =>
    set((s) => ({
      rules: s.rules.map((r) => (r.id === id ? { ...r, ...patch } : r)),
    })),

  removeRule: (id) =>
    set((s) => ({ rules: s.rules.filter((r) => r.id !== id) })),

  moveRule: (id, delta) =>
    set((s) => {
      const index = s.rules.findIndex((r) => r.id === id);
      const target = index + delta;
      if (index < 0 || target < 0 || target >= s.rules.length) return s;
      const next = [...s.rules];
      const [moved] = next.splice(index, 1);
      next.splice(target, 0, moved);
      return { rules: next };
    }),

  clearRules: () => set({ rules: [] }),
  setRules: (rules) => set({ rules }),

  toggleSelect: (id) =>
    set((s) => ({
      selected: s.selected.includes(id)
        ? s.selected.filter((x) => x !== id)
        : [...s.selected, id],
    })),

  selectAll: (ids) =>
    set((s) => ({ selected: ids ?? s.entries.map((e) => e.id) })),
  selectNone: () => set({ selected: [] }),
  selectInvert: (ids) =>
    set((s) => {
      const candidates = ids ?? s.entries.map((e) => e.id);
      const sel = new Set(s.selected);
      return { selected: candidates.filter((id) => !sel.has(id)) };
    }),

  setPolicy: (policy) => set({ policy }),

  rescanSoon: () => {
    if (rescanTimer !== null) window.clearTimeout(rescanTimer);
    rescanTimer = window.setTimeout(() => {
      rescanTimer = null;
      if (!get().dir.trim()) return;
      void get().scan(undefined, true);
    }, 450);
  },

  clearList: () => {
    set({ sessionId: null, entries: [], selected: [], plan: null });
    // 顺手让后端释放会话，别把上一次的条目一直留在内存里
    void api.clearSession().catch(() => {});
  },

  refresh: async () => {
    const { sessionId, rules, selected, entries, policy } = get();
    if (!sessionId) return;
    if (selected.length === 0) {
      set({ plan: null });
      return;
    }
    try {
      const plan = await api.preview({
        sessionId,
        ids: selected.length === entries.length ? null : selected,
        rules,
        conflictPolicy: policy,
      });
      set({ plan });
    } catch (e) {
      get().notify(errorText(e), "err");
    }
  },

  run: async () => {
    const { sessionId, rules, selected, entries, policy, plan, dir } = get();
    if (!sessionId || !plan || !plan.canExecute) return;

    set({ executing: true });
    try {
      const res = await api.execute({
        sessionId,
        ids: selected.length === entries.length ? null : selected,
        rules,
        conflictPolicy: policy,
      });
      // 结果统一在重扫之后弹一次，避免先闪一条短提示再被带按钮的提示顶掉
      // 文件模式：文件名已经变了，把清单同步成新路径，否则重扫时旧路径全部找不到
      if (get().mode === "files") {
        const byPath = new Map(
          get().entries.map((e) => [joinPath(e.dir, e.name), e])
        );
        const items = new Map(plan.items.map((i) => [i.id, i]));
        const moved: { from: string; to: string }[] = [];
        const nextFiles = get().files.map((f) => {
          const entry = byPath.get(f);
          const item = entry ? items.get(entry.id) : undefined;
          if (!entry || !item || !item.changed) return f;
          const to = joinPath(entry.dir, item.dstName);
          moved.push({ from: f, to });
          return to;
        });
        set({ files: nextFiles, renamedFiles: moved });
      }
      // 只记在本次会话里：应用关掉后这条记录就没了，撤销也就没有入口
      set({
        history: [
          {
            id: res.operationId,
            dir,
            startedAt: Date.now(),
            total: res.total,
            renamed: res.renamed,
            failed: res.failed.length,
            state: res.state,
            canUndo: res.state !== "interrupted" && res.renamed > 0,
          },
          ...get().history,
        ],
      });
      await get().scan(dir);

      const failedCount = res.failed.length;
      const undoable = res.state !== "interrupted" && res.renamed > 0;

      if (undoable) {
        const secs = 12;
        get().showToast({
          kind: failedCount > 0 ? "info" : "ok",
          text:
            (failedCount > 0
              ? `已重命名 ${res.renamed} 项，${failedCount} 项失败`
              : `已重命名 ${res.renamed} 项`) +
            ` · ${secs} 秒内可点「撤销」直接还原，提示关闭后请到「历史记录」撤销`,
          actionLabel: "撤销",
          action: () => {
            get().clearToast();
            void get().undoOperation(res.operationId);
          },
          seconds: secs,
        });
        // 顺手把历史记录打开，让这一条立刻可见
        void get().toggleHistory(true);
      } else {
        get().notify(
          failedCount > 0
            ? `已重命名 ${res.renamed} 项，${failedCount} 项失败：${res.failed[0].reason}`
            : "这次执行没有实际变化",
          failedCount > 0 ? "err" : "info"
        );
      }
    } catch (e) {
      get().notify(errorText(e), "err");
    } finally {
      set({ executing: false });
    }
  },

  undoOperation: async (operationId) => {
    try {
      const res = await api.undo(operationId);
      const failed = res.failed.length;
      get().notify(
        failed > 0
          ? `已还原 ${res.restored} 项，${failed} 项未能还原`
          : `已还原 ${res.restored} 项`,
        failed > 0 ? "info" : "ok"
      );
      // 文件模式下撤销会把名字改回原名，清单也要跟着回退，否则重扫找不到文件
      if (get().mode === "files" && get().renamedFiles.length > 0) {
        const back = new Map(get().renamedFiles.map((m) => [m.to, m.from]));
        set({
          files: get().files.map((f) => back.get(f) ?? f),
          renamedFiles: [],
        });
      }
      // 这一条已经回滚过了，就不该再提供撤销入口
      set({
        history: get().history.map((h) =>
          h.id === operationId
            ? { ...h, state: "rolledBack" as const, canUndo: false }
            : h
        ),
      });
      await get().scan(get().dir);
    } catch (e) {
      get().notify(errorText(e), "err");
    }
  },

  undoLast: async () => {
    const latest = get().history.find((o) => o.canUndo);
    if (!latest) {
      get().notify("本次打开还没有可撤销的操作", "info");
      return;
    }
    await get().undoOperation(latest.id);
  },

  toggleHistory: async (open) => {
    // 历史只存在内存里，打开抽屉不用再问后端
    set({ historyOpen: open ?? !get().historyOpen });
  },

  loadPresets: async () => {
    try {
      const file = await api.getPresets();
      set({ presets: file.presets });
    } catch {
      // 预设是附加能力，读失败不打断主流程
    }
  },

  savePreset: async (name) => {
    const rules = get().rules;
    if (rules.length === 0) {
      get().notify("当前没有规则可保存", "err");
      return;
    }
    const preset: Preset = {
      id: `p${Date.now().toString(36)}`,
      name,
      rules,
      created: Date.now(),
    };
    const presets = [...get().presets, preset];
    set({ presets });
    try {
      await api.savePresets({ presets });
      get().notify(`已保存预设「${name}」`, "ok");
    } catch (e) {
      get().notify(errorText(e), "err");
    }
  },

  applyPreset: (id) => {
    const preset = get().presets.find((p) => p.id === id);
    if (!preset) return;
    set({ rules: preset.rules.map((r) => ({ ...r, id: newRuleId() })) });
    get().notify(`已应用预设「${preset.name}」`, "ok");
  },

  deletePreset: async (id) => {
    const presets = get().presets.filter((p) => p.id !== id);
    set({ presets });
    try {
      await api.savePresets({ presets });
    } catch (e) {
      get().notify(errorText(e), "err");
    }
  },

  showToast: (t) => {
    toastSeq += 1;
    set({ toast: { ...t, id: toastSeq } });
  },
  notify: (text, kind = "info") => get().showToast({ text, kind }),
  clearToast: () => set({ toast: null }),
}));

/** 从计划里取出某个条目，供表格复用 */
export function findPlanItem(plan: PreviewResult | null, id: number): PlanItem | undefined {
  return plan?.items.find((i) => i.id === id);
}
