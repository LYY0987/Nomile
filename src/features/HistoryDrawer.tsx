import { History, Undo2, X } from "lucide-react";

import type { OperationState, OperationSummary } from "../lib/types";
import { useApp } from "../store/appStore";

const STATE_LABEL: Record<OperationState, string> = {
  running: "进行中",
  completed: "已完成",
  partial: "部分完成",
  rolledBack: "已撤销",
  interrupted: "已中断",
};

const STATE_CLASS: Record<OperationState, string> = {
  running: "mute",
  completed: "ok",
  partial: "warn",
  rolledBack: "mute",
  interrupted: "err",
};

function formatTime(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ` +
    `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
  );
}

function shortDir(dir: string): string {
  const parts = dir.split(/[\\/]/).filter(Boolean);
  if (parts.length <= 2) return dir;
  return `…\\${parts.slice(-2).join("\\")}`;
}

export function HistoryDrawer() {
  const open = useApp((s) => s.historyOpen);
  const history = useApp((s) => s.history);
  const toggle = useApp((s) => s.toggleHistory);
  const undoOperation = useApp((s) => s.undoOperation);

  if (!open) return null;

  return (
    <div className="drawer-mask" onClick={() => void toggle(false)}>
      <div className="drawer" onClick={(e) => e.stopPropagation()}>
        <div className="drawer-head">
          <History />
          <span>历史记录</span>
          <div className="spacer" />
          <button className="btn icon" onClick={() => void toggle(false)}>
            <X />
          </button>
        </div>

        <div className="drawer-body">
          {history.length === 0 && (
            <div className="empty">
              <div className="big">本次打开还没有执行过重命名</div>
              <div className="sub">
                撤销只在当前会话内有效：关掉应用后重新打开，这里会从空白开始。
              </div>
            </div>
          )}

          {history.map((item) => (
            <HistItem
              key={item.id}
              item={item}
              onUndo={() => void undoOperation(item.id)}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function HistItem({
  item,
  onUndo,
}: {
  item: OperationSummary;
  onUndo: () => void;
}) {
  return (
    <div className="hist-item">
      <div className="hist-top">
        <span className="hist-time">{formatTime(item.startedAt)}</span>
        <span className={`tag ${STATE_CLASS[item.state]}`}>
          {STATE_LABEL[item.state]}
        </span>
        <div className="spacer" />
        <button
          className="btn small"
          style={{ height: 24 }}
          disabled={!item.canUndo}
          onClick={onUndo}
        >
          <Undo2 />
          撤销
        </button>
      </div>
      <div className="hist-dir" title={item.dir}>
        {shortDir(item.dir)}
      </div>
      <div className="hist-meta">
        共 {item.total} 项 · 已改名 {item.renamed} 项
        {item.failed > 0 ? ` · 失败 ${item.failed} 项` : ""}
      </div>
    </div>
  );
}
