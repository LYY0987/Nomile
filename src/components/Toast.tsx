import { CircleAlert, CircleCheck, Info } from "lucide-react";
import { useEffect } from "react";

import { useApp } from "../store/appStore";

const ICON = {
  ok: CircleCheck,
  err: CircleAlert,
  info: Info,
} as const;

export function Toast() {
  const toast = useApp((s) => s.toast);
  const clearToast = useApp((s) => s.clearToast);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(clearToast, 3600);
    return () => window.clearTimeout(timer);
  }, [toast, clearToast]);

  if (!toast) return null;

  const Icon = ICON[toast.kind];

  return (
    <div className={`toast ${toast.kind}`} onClick={clearToast}>
      <Icon />
      {toast.text}
    </div>
  );
}
