import { open } from "@tauri-apps/plugin-dialog";
import { Files, FolderOpen, History, Moon, RefreshCw, Sun } from "lucide-react";

import { useApp } from "../store/appStore";

function fileName(path: string): string {
  const i = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return i >= 0 ? path.slice(i + 1) : path;
}

/**
 * 顶部只做一件事：决定「处理哪些东西」。
 * 上半行是来源（目录 / 一批文件），下半行是这个来源的扫描范围条件。
 * 排序、搜索、批量选择属于列表自己的事，放在列表上方，不混在这里。
 */
export function Toolbar() {
  const dir = useApp((s) => s.dir);
  const mode = useApp((s) => s.mode);
  const files = useApp((s) => s.files);
  const options = useApp((s) => s.options);
  const setSource = useApp((s) => s.setSource);
  const applyOptions = useApp((s) => s.applyOptions);
  const scan = useApp((s) => s.scan);
  const scanning = useApp((s) => s.scanning);
  const toggleHistory = useApp((s) => s.toggleHistory);
  const theme = useApp((s) => s.theme);
  const toggleTheme = useApp((s) => s.toggleTheme);
  const notify = useApp((s) => s.notify);

  const dirMode = mode === "dir";
  const canLoad = dirMode ? !!dir.trim() : files.length > 0;

  async function pickDir() {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "选择要整理的目录",
      });
      if (typeof picked === "string") {
        setSource({ mode: "dir", dir: picked, files: [] });
        await scan(picked);
      }
    } catch (e) {
      notify(String(e), "err");
    }
  }

  async function pickFiles() {
    try {
      const picked = await open({
        directory: false,
        multiple: true,
        title: "选择要处理的文件",
      });
      const list = Array.isArray(picked) ? picked : picked ? [picked] : [];
      if (list.length === 0) return;
      setSource({ mode: "files", files: list, dir: "" });
      await scan();
    } catch (e) {
      notify(String(e), "err");
    }
  }

  return (
    <div className="toolbar">
      <div className="toolbar-row">
        <span className="row-label">来源</span>
        <div className="path-box">
          {dirMode ? (
            <input
              value={dir}
              spellCheck={false}
              placeholder="选择或粘贴一个目录路径"
              onChange={(e) => setSource({ mode: "dir", dir: e.currentTarget.value })}
              onKeyDown={(e) => {
                if (e.key === "Enter") void scan();
              }}
            />
          ) : (
            <span className="path-text" title={files.join("\n")}>
              {files.length > 0
                ? `已选择 ${files.length} 个文件 · ${fileName(files[0])}${
                    files.length > 1 ? " 等" : ""
                  }`
                : "还没有选择文件"}
            </span>
          )}
          {dirMode && dir && <span className="hint">Enter 载入</span>}
        </div>
        <button className="btn" onClick={() => void pickDir()}>
          <FolderOpen />
          选目录
        </button>
        <button className="btn" onClick={() => void pickFiles()}>
          <Files />
          选文件
        </button>
        <button
          className="btn primary"
          onClick={() => void scan()}
          disabled={scanning || !canLoad}
          title="按当前来源与范围条件载入列表"
        >
          <RefreshCw className={scanning ? "spin" : undefined} />
          {scanning ? "载入中…" : "载入列表"}
        </button>
        <div className="spacer" />
        <button className="btn" onClick={() => void toggleHistory(true)}>
          <History />
          历史记录
        </button>
        <button
          className="btn icon"
          title={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"}
          onClick={toggleTheme}
        >
          {theme === "dark" ? <Sun /> : <Moon />}
        </button>
      </div>

      <div className={`toolbar-row scope-row${dirMode ? "" : " is-muted"}`}>
        <span className="row-label">范围</span>
        <label className="check">
          <input
            type="checkbox"
            checked={options.recursive}
            disabled={!dirMode}
            onChange={(e) => applyOptions({ recursive: e.currentTarget.checked })}
          />
          含子目录
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={options.includeHidden}
            disabled={!dirMode}
            onChange={(e) => applyOptions({ includeHidden: e.currentTarget.checked })}
          />
          含隐藏项
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={options.includeDirs}
            disabled={!dirMode}
            onChange={(e) => applyOptions({ includeDirs: e.currentTarget.checked })}
          />
          含文件夹
        </label>
        <input
          className="input small"
          style={{ width: 190 }}
          disabled={!dirMode}
          placeholder="只看这些，如 *.jpg, *.png"
          value={options.includePattern}
          onChange={(e) => applyOptions({ includePattern: e.currentTarget.value })}
          title="按文件名匹配，支持 * 和 ?；多个模式用逗号分隔；留空表示全部"
        />
        <input
          className="input small"
          style={{ width: 160 }}
          disabled={!dirMode}
          placeholder="排除，如 *.tmp"
          value={options.excludePattern}
          onChange={(e) => applyOptions({ excludePattern: e.currentTarget.value })}
          title="按文件名匹配，支持 * 和 ?；多个模式用逗号分隔"
        />
        <div className="spacer" />
        <span className="hint">
          {dirMode ? "改条件会立即重新载入" : "已直接指定文件，范围条件不适用"}
        </span>
      </div>
    </div>
  );
}
