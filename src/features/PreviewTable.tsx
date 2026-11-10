import {
  ArrowUpDown,
  CheckCheck,
  FolderOpen,
  List,
  Loader,
  Play,
  Repeat,
  Search,
  Square,
  Trash2,
  Undo2,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";

import type { ConflictPolicy, Issue, PlanItem, SortBy } from "../lib/types";
import { useApp } from "../store/appStore";

const ROW_H = 32;

const SORTS: { value: SortBy; label: string }[] = [
  { value: "name", label: "名称" },
  { value: "modified", label: "修改时间" },
  { value: "created", label: "创建时间" },
  { value: "size", label: "大小" },
  { value: "path", label: "路径" },
];

/** 状态列只放得下几个字，按 issue 类型给短标签，完整原因放 tooltip */
function warnLabel(issue: Issue): string {
  switch (issue.kind) {
    case "existsOnDisk":
      // 自动加序号时会走到这里；策略为跳过时是 Info 级，由状态列另行判断
      return "重名避让";
    case "tooLong":
      return "路径超长";
    case "metaMissing":
      return "缺元数据";
    default:
      return issue.message.slice(0, 8);
  }
}

function errorLabel(issue: Issue): string {
  switch (issue.kind) {
    case "emptyName":
      return "名字为空";
    case "illegalChar":
      return "含非法字符";
    case "reservedName":
      return "系统保留名";
    case "duplicate":
      return "与同批重名";
    default:
      return issue.message.slice(0, 8);
  }
}

/**
 * b 的每个字符能否按顺序在 a 里找到（允许 a 多出字符）
 */
function isSubsequence(a: string[], b: string[]): boolean {
  let i = 0;
  for (let j = 0; j < b.length; j += 1) {
    while (i < a.length && a[i] !== b[j]) i += 1;
    if (i >= a.length) return false;
    i += 1;
  }
  return true;
}

/** 拆出名字主体和扩展名，规则与后端一致：隐藏文件、末尾点都视为没有扩展名 */
function splitExtChars(name: string[]): [string[], string[]] {
  let dot = -1;
  for (let i = name.length - 1; i >= 0; i -= 1) {
    if (name[i] === ".") {
      dot = i;
      break;
    }
  }
  if (dot <= 0 || dot === name.length - 1) return [name, []];
  return [name.slice(0, dot), name.slice(dot)];
}

function indexOfSeq(hay: string[], needle: string[]): number {
  if (needle.length === 0) return -1;
  for (let i = 0; i + needle.length <= hay.length; i += 1) {
    let hit = true;
    for (let j = 0; j < needle.length; j += 1) {
      if (hay[i + j] !== needle[j]) {
        hit = false;
        break;
      }
    }
    if (hit) return i;
  }
  return -1;
}

/**
 * 算出新名里真正变化的区间，最多两段。
 *
 * 首选判断「名字主体整段还在不在」：原名的主体（不含扩展名）在新名里原样出现时，
 * 只有它前后新增的文字该上色，中间那段原名字符保持原样。
 * 主体被改掉了，才退回「公共前缀 + 公共后缀」的近似算法。
 *
 * 不能只算公共前后缀：`IMG_001.png → IMGIMG_001_001.png` 时，
 * 公共后缀会一路吃到中间，把差异挤到名字正中，前缀反而看不出变化。
 */
function diffMarks(a: string, b: string): [number, number][] {
  const ac = Array.from(a);
  const bc = Array.from(b);

  // 纯删除（新名只是原名的子序列）时新名里没有任何「新增」字符，
  // 硬标一段会把保留下来的文字误标成新内容，所以干脆不标。
  if (isSubsequence(ac, bc)) return [];

  const [aStem, aExt] = splitExtChars(ac);
  const [bStem, bExt] = splitExtChars(bc);
  const sameExt = aExt.join("") === bExt.join("");

  // 1. 原名的名字主体原样出现在新名里 → 只标它前后新增的部分
  const at = indexOfSeq(bStem, aStem);
  if (aStem.length > 0 && at >= 0) {
    const marks: [number, number][] = [];
    if (at > 0) marks.push([0, at]);

    const tail = at + aStem.length;
    // 扩展名也变了就一路标到结尾，让扩展名的改动也可见
    const end = sameExt ? bStem.length : bc.length;
    if (end > tail) {
      const last = marks[marks.length - 1];
      if (last && last[1] === tail) last[1] = end;
      else marks.push([tail, end]);
    }

    if (marks.length > 0) return marks;
  }

  // 2. 退化：公共前缀 + 公共后缀
  let p = 0;
  const maxP = Math.min(ac.length, bc.length);
  while (p < maxP && ac[p] === bc[p]) p += 1;

  let s = 0;
  const maxS = Math.min(ac.length, bc.length) - p;
  while (s < maxS && ac[ac.length - 1 - s] === bc[bc.length - 1 - s]) s += 1;

  const bStart = p;
  const bEnd = bc.length - s;
  return bEnd > bStart ? [[bStart, bEnd]] : [];
}

function Marked({ text, marks }: { text: string; marks: [number, number][] }) {
  // Rust 的索引是 char 索引，必须按 code point 切，不能直接用 UTF-16 下标
  const chars = Array.from(text);
  const valid = marks.filter(
    ([s, e]) => s >= 0 && e <= chars.length && s < e
  );
  if (valid.length === 0) return <>{text}</>;

  const nodes: ReactNode[] = [];
  let cursor = 0;
  valid.forEach(([s, e], i) => {
    if (s > cursor) {
      nodes.push(<span key={`p${i}`}>{chars.slice(cursor, s).join("")}</span>);
    }
    nodes.push(<mark key={`m${i}`}>{chars.slice(s, e).join("")}</mark>);
    cursor = e;
  });
  if (cursor < chars.length) {
    nodes.push(<span key="tail">{chars.slice(cursor).join("")}</span>);
  }
  return <>{nodes}</>;
}

export function PreviewTable() {
  const entries = useApp((s) => s.entries);
  const selected = useApp((s) => s.selected);
  const plan = useApp((s) => s.plan);
  const dir = useApp((s) => s.dir);
  const sessionId = useApp((s) => s.sessionId);
  const policy = useApp((s) => s.policy);
  const setPolicy = useApp((s) => s.setPolicy);
  const toggleSelect = useApp((s) => s.toggleSelect);
  const selectAll = useApp((s) => s.selectAll);
  const selectNone = useApp((s) => s.selectNone);
  const selectInvert = useApp((s) => s.selectInvert);
  const run = useApp((s) => s.run);
  const undoLast = useApp((s) => s.undoLast);
  const executing = useApp((s) => s.executing);
  const scanning = useApp((s) => s.scanning);

  // 列表自己的条件：排序走「载入参数」（会影响 ${n} 的取值），搜索只影响这一屏显示
  const options = useApp((s) => s.options);
  const applyOptions = useApp((s) => s.applyOptions);
  const query = useApp((s) => s.query);
  const setQuery = useApp((s) => s.setQuery);
  const clearList = useApp((s) => s.clearList);

  const planMap = useMemo(() => {
    const map = new Map<number, PlanItem>();
    plan?.items.forEach((item) => map.set(item.id, item));
    return map;
  }, [plan]);

  const selectedSet = useMemo(() => new Set(selected), [selected]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return entries;
    return entries.filter((e) => e.name.toLowerCase().includes(q));
  }, [entries, query]);

  const visibleIds = useMemo(() => visible.map((e) => e.id), [visible]);

  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewHeight, setViewHeight] = useState(600);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const measure = () => setViewHeight(el.clientHeight);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const start = Math.max(0, Math.floor(scrollTop / ROW_H) - 4);
  const count = Math.ceil(viewHeight / ROW_H) + 8;
  const end = Math.min(visible.length, start + count);

  let visibleSelected = 0;
  for (const e of visible) if (selectedSet.has(e.id)) visibleSelected += 1;
  const allSelected = visible.length > 0 && visibleSelected === visible.length;

  if (!sessionId) {
    return (
      <div className="main">
        <div className="empty">
          <FolderOpen size={30} className="empty-icon" />
          <div className="big">{dir ? "列表已清空" : "选一个目录或一批文件开始"}</div>
          <div className="sub">
            {dir
              ? `点上方「载入列表」可以重新载入 ${dir}`
              : `左侧添加规则后，这里会实时显示每个文件改名前后的对比；
            只有确认无误，点右下角的「执行重命名」才会真正改磁盘上的名字。`}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="main">
      <div className="list-bar">
        <span className="row-label">
          <List />
          列表
        </span>
        <ArrowUpDown style={{ width: 14, height: 14, color: "var(--text-faint)" }} />
        <select
          className="input small"
          value={options.sortBy}
          onChange={(e) => applyOptions({ sortBy: e.currentTarget.value as SortBy })}
          title="排序依据（会重新载入，保证序号与预览一致）"
        >
          {SORTS.map((s) => (
            <option key={s.value} value={s.value}>
              按{s.label}
            </option>
          ))}
        </select>
        <label className="check">
          <input
            type="checkbox"
            checked={options.sortDesc}
            onChange={(e) => applyOptions({ sortDesc: e.currentTarget.checked })}
          />
          降序
        </label>

        <div className="search-box">
          <Search />
          <input
            placeholder="按文件名搜索"
            value={query}
            onChange={(e) => setQuery(e.currentTarget.value)}
            title="只过滤这一屏的显示，不影响已勾选的执行范围"
          />
        </div>
        {query.trim() !== "" && (
          <span className="hint">匹配 {visible.length} / {entries.length}</span>
        )}

        <div className="spacer" />

        <button
          className="btn small"
          onClick={() => selectAll(visibleIds)}
          disabled={visible.length === 0}
        >
          <CheckCheck />
          全选
        </button>
        <button
          className="btn small"
          onClick={selectNone}
          disabled={selected.length === 0}
        >
          <Square />
          取消全选
        </button>
        <button
          className="btn small"
          onClick={() => selectInvert(visibleIds)}
          disabled={visible.length === 0}
        >
          <Repeat />
          反选
        </button>
        <button
          className="btn small"
          onClick={clearList}
          disabled={entries.length === 0}
          title="只清空界面，不动磁盘文件"
        >
          <Trash2 />
          清空列表
        </button>
      </div>

      <div className="table-head">
        <input
          type="checkbox"
          checked={allSelected}
          ref={(el) => {
            // 只选中一部分时显示 indeterminate，别让表头看起来是「没选」
            if (el) el.indeterminate = visibleSelected > 0 && !allSelected;
          }}
          onChange={() => (allSelected ? selectNone() : selectAll(visibleIds))}
          title={allSelected ? "取消全选" : "全选"}
        />
        <span className="col">原文件名</span>
        <span />
        <span className="col">新文件名</span>
        <span className="col">状态</span>
      </div>

      {entries.length === 0 ? (
        <div className="empty">
          <div className="big">没有可处理的条目</div>
          <div className="sub">{dir}</div>
        </div>
      ) : (
        <div
          className="table-scroll"
          ref={scrollRef}
          onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
        >
          <div className="rows" style={{ height: visible.length * ROW_H }}>
            {visible.slice(start, end).map((entry, offset) => {
              const index = start + offset;
              const item = planMap.get(entry.id);
              const isSelected = selectedSet.has(entry.id);
              const errIssue = item?.issues.find((i) => i.level === "error");
              const warnIssue = item?.issues.find((i) => i.level === "warning");
              const infoIssue = item?.issues.find((i) => i.level === "info");

              return (
                <div
                  key={entry.id}
                  className={`row${isSelected ? " is-selected" : ""}${
                    errIssue ? " is-error" : ""
                  }`}
                  style={{ top: index * ROW_H }}
                  title={item?.issues.map((i) => i.message).join("；")}
                  onClick={(e) => {
                    // 复选框自己会触发一次，别让它再冒泡上来切第二次
                    if ((e.target as HTMLElement).tagName === "INPUT") return;
                    toggleSelect(entry.id);
                  }}
                >
                  <input
                    type="checkbox"
                    checked={isSelected}
                    onChange={() => toggleSelect(entry.id)}
                  />
                  <span className="name src">{entry.name}</span>
                  <span className="arrow">→</span>
                  <span className="name">
                    {!isSelected ? (
                      <span style={{ color: "var(--text-faint)" }}>—</span>
                    ) : item ? (
                      item.changed ? (
                        <Marked
                          text={item.dstName}
                          marks={diffMarks(item.srcName, item.dstName)}
                        />
                      ) : (
                        <span style={{ color: "var(--text-faint)" }}>
                          {item.dstName}
                        </span>
                      )
                    ) : (
                      <span style={{ color: "var(--text-faint)" }}>—</span>
                    )}
                  </span>
                  <span className="status">
                    {!isSelected ? (
                      <span className="tag mute">未选择</span>
                    ) : errIssue ? (
                      <span className="tag err" title={errIssue.message}>
                        {errorLabel(errIssue)}
                      </span>
                    ) : warnIssue ? (
                      <span className="tag warn" title={warnIssue.message}>
                        {warnLabel(warnIssue)}
                      </span>
                    ) : infoIssue?.kind === "caseOnly" ? (
                      <span className="tag mute" title={infoIssue.message}>
                        仅改大小写
                      </span>
                    ) : infoIssue?.kind === "existsOnDisk" ? (
                      <span className="tag mute" title={infoIssue.message}>
                        重名跳过
                      </span>
                    ) : item?.changed ? (
                      <span className="tag ok">待执行</span>
                    ) : (
                      <span className="tag mute">未改动</span>
                    )}
                  </span>
                </div>
              );
            })}
          </div>
        </div>
      )}

      <div className="footer">
        <span className="stat">
          共 <b>{entries.length}</b> 项 · 选中 <b>{selected.length}</b>
        </span>
        <span className="stat">
          待执行 <b>{plan?.changed ?? 0}</b> 项
        </span>
        {plan && plan.errors > 0 && (
          <span className="stat err">
            错误 <b>{plan.errors}</b>
          </span>
        )}
        {plan && plan.warnings > 0 && (
          <span className="stat warn">
            警告 <b>{plan.warnings}</b>
          </span>
        )}

        <div className="spacer" />

        <label className="check">
          目标重名时
          <select
            className="input small"
            value={policy}
            onChange={(e) => setPolicy(e.currentTarget.value as ConflictPolicy)}
          >
            <option value="autoSuffix">自动加序号</option>
            <option value="skip">跳过该文件</option>
          </select>
        </label>

        <button
          className="btn"
          onClick={() => void undoLast()}
          title="撤销本次打开里最近一次改名操作"
        >
          <Undo2 />
          撤销修改
        </button>
        <button
          className="btn primary"
          disabled={!plan?.canExecute || executing || scanning}
          onClick={() => void run()}
          title={
            plan && plan.errors > 0
              ? "存在错误项，需先修正"
              : plan && plan.changed === 0
                ? "没有发生变化的条目"
                : "执行重命名"
          }
        >
          {executing ? <Loader className="spin" /> : <Play />}
          {executing
            ? "执行中…"
            : `执行重命名${plan?.changed ? `（${plan.changed}）` : ""}`}
        </button>
      </div>
    </div>
  );
}
