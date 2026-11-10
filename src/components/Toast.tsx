import { CircleAlert, CircleCheck, Info, Undo2 } from "lucide-react";
import { useEffect, useState } from "react";

import { useApp } from "../store/appStore";

const ICON = {
  ok: CircleCheck,
  err: CircleAlert,
  info: Info,
} as const;

/** 没指定 seconds 时用这个，跟原来的自动消失时长一致 */
const DEFAULT_MS = 3600;

export function Toast() {
  const toast = useApp((s) => s.toast);
  const clearToast = useApp((s) => s.clearToast);

  const totalMs = toast?.seconds ? toast.seconds * 1000 : DEFAULT_MS;
  const toastId = toast?.id ?? 0;
  const [leftMs, setLeftMs] = useState(totalMs);

  useEffect(() => {
    if (!toastId) return;
    const startedAt = Date.now();
    setLeftMs(totalMs);

    const tick = () =>
      setLeftMs(Math.max(0, totalMs - (Date.now() - startedAt)));
    const ticker = window.setInterval(tick, 100);
    const closer = window.setTimeout(clearToast, totalMs);

    return () => {
      window.clearInterval(ticker);
      window.clearTimeout(closer);
    };
    // toastId 换一次就重置一次，totalMs 变了也要重算
  }, [toastId, totalMs, clearToast]);

  if (!toast) return null;

  const Icon = ICON[toast.kind];
  const withCountdown = Boolean(toast.seconds);
  const percent =
    totalMs > 0 ? Math.max(0, Math.min(100, (leftMs / totalMs) * 100)) : 0;

  return (
    <div
      className={`toast ${toast.kind}${toast.action ? " has-action" : ""}`}
      onClick={clearToast}
      title={withCountdown ? "点击可立即关闭" : undefined}
    >
      <Icon />
      <span className="toast-text">{toast.text}</span>

      {toast.action && (
        <button
          className="toast-action"
          onClick={(e) => {
            // 别让点击冒泡去触发整条的「关闭」
            e.stopPropagation();
            toast.action?.();
          }}
        >
          <Undo2 />
          {toast.actionLabel ?? "撤销"}
        </button>
      )}

      {withCountdown && (
        <span className="toast-count">{Math.ceil(leftMs / 1000)}s</span>
      )}

      {withCountdown && (
        <span className="toast-progress" style={{ width: `${percent}%` }} />
      )}
    </div>
  );
}
