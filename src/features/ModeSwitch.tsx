import { Bot, List } from "lucide-react";

import { useApp } from "../store/appStore";

/** 左栏顶部：手动规则 / AI 重命名 二选一 */
export function ModeSwitch() {
  const mode = useApp((s) => s.panelMode);
  const setMode = useApp((s) => s.setPanelMode);

  return (
    <div className="mode-switch">
      <button
        className={mode === "rules" ? "is-active" : ""}
        onClick={() => setMode("rules")}
      >
        <List />
        手动规则
      </button>
      <button
        className={mode === "ai" ? "is-active" : ""}
        onClick={() => setMode("ai")}
      >
        <Bot />
        AI 重命名
      </button>
    </div>
  );
}
