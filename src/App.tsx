import { useEffect } from "react";

import { Toast } from "./components/Toast";
import { AiPanel } from "./features/AiPanel";
import { HistoryDrawer } from "./features/HistoryDrawer";
import { PreviewTable } from "./features/PreviewTable";
import { RulePanel } from "./features/RulePanel";
import { Toolbar } from "./features/Toolbar";
import { api } from "./lib/ipc";
import { useApp } from "./store/appStore";

export default function App() {
  const rules = useApp((s) => s.rules);
  const selected = useApp((s) => s.selected);
  const policy = useApp((s) => s.policy);
  const sessionId = useApp((s) => s.sessionId);
  const panelMode = useApp((s) => s.panelMode);
  const aiSpec = useApp((s) => s.aiSpec);
  const scanning = useApp((s) => s.scanning);
  const executing = useApp((s) => s.executing);

  // 规则 / 选中 / 冲突策略 / 面板任何一处变化，250ms 后重新算一遍预览。
  // 计算在 Rust 里是纯函数，1 万条也就几十毫秒，防抖只是为了别让输入卡顿。
  useEffect(() => {
    if (!sessionId) return;
    const timer = window.setTimeout(() => {
      void useApp.getState().refresh();
    }, 250);
    return () => window.clearTimeout(timer);
  }, [rules, selected, policy, sessionId, panelMode, aiSpec]);

  // 启动时只恢复「载入条件」和冲突策略这类设置。
  // 目录和列表一律从空白开始：不恢复上次的目录，也不延续上次的撤销记录。
  useEffect(() => {
    void (async () => {
      await useApp.getState().loadPresets();
      try {
        const settings = await api.getSettings();
        useApp.setState({
          options: {
            recursive: settings.recursive,
            includeHidden: settings.includeHidden,
            includeDirs: settings.includeDirs,
            includePattern: "",
            excludePattern: "",
            sortBy: settings.sortBy,
            sortDesc: settings.sortDesc,
          },
          policy: settings.conflictPolicy,
        });
      } catch {
        // 首次运行没有配置文件，正常
      }
    })();
  }, []);

  return (
    <div className="app">
      {(scanning || executing) && <div className="busy-line" />}
      <Toolbar />
      <div className="body">
        {panelMode === "ai" ? <AiPanel /> : <RulePanel />}
        <PreviewTable />
      </div>
      <HistoryDrawer />
      <Toast />
    </div>
  );
}
