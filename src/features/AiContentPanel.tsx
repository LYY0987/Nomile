import {
  AlertTriangle,
  Check,
  FileText,
  Loader,
  RefreshCw,
  Sparkles,
  Square,
  X,
} from "lucide-react";

import type { AiMapResultEntry } from "../lib/types";
import { useAi } from "../store/aiStore";
import { useApp } from "../store/appStore";

const EXAMPLES = [
  "按文档主题起一个简洁的标题",
  "提取邮件标题作为文件名",
  "用代码文件的主要功能命名",
  "用笔记的第一句话概括",
];

function withExt(srcName: string, stem: string): string {
  const dot = srcName.lastIndexOf(".");
  return dot > 0 ? `${stem}${srcName.slice(dot)}` : stem;
}

function extOf(srcName: string): string {
  const dot = srcName.lastIndexOf(".");
  return dot > 0 ? srcName.slice(dot) : "";
}

function mappingKey(entry: AiMapResultEntry): string {
  return `${entry.id}|${entry.stem}`;
}

export function AiContentPanel() {
  const sessionId = useApp((s) => s.sessionId);
  const aiMap = useApp((s) => s.aiMap);

  const providers = useAi((s) => s.providers);
  const activeId = useAi((s) => s.activeId);
  const contentDraft = useAi((s) => s.contentDraft);
  const setContentDraft = useAi((s) => s.setContentDraft);
  const allowContent = useAi((s) => s.allowContent);
  const setAllowContent = useAi((s) => s.setAllowContent);
  const generating = useAi((s) => s.generating);
  const status = useAi((s) => s.status);
  const streamText = useAi((s) => s.streamText);
  const error = useAi((s) => s.error);
  const contentResult = useAi((s) => s.contentResult);
  const generateContent = useAi((s) => s.generateContent);
  const cancel = useAi((s) => s.cancel);
  const applyContent = useAi((s) => s.applyContent);
  const updateContentEntry = useAi((s) => s.updateContentEntry);
  const clearContent = useAi((s) => s.clearContent);

  const active = providers.find((p) => p.id === activeId);
  const canGenerate =
    !!sessionId &&
    contentDraft.trim().length > 0 &&
    allowContent &&
    !generating;

  const applied =
    !!contentResult &&
    !!aiMap &&
    aiMap.length === contentResult.entries.length &&
    contentResult.entries.every((entry) =>
      aiMap.some((item) => mappingKey(entry) === `${item.id}|${item.stem}`)
    );

  return (
    <div className="ai-scroll">
      {!sessionId && (
        <div className="ai-hint">
          先在上方载入一批文件，按内容命名只处理选中的文本、PDF、DOCX、PPTX 文件。
        </div>
      )}

      <div className="ai-card">
        <div className="ai-label">
          <FileText />
          根据文件内容逐文件起名
        </div>
        <textarea
          className="ai-textarea"
          value={contentDraft}
          placeholder="例如：按文档主题起一个简洁的标题，保持中文"
          onChange={(event) => setContentDraft(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
              event.preventDefault();
              if (canGenerate) void generateContent(contentDraft);
            }
          }}
        />
        <div className="ai-examples">
          {EXAMPLES.map((example) => (
            <button
              key={example}
              className="ai-example"
              onClick={() => setContentDraft(example)}
            >
              {example}
            </button>
          ))}
        </div>

        <div className="ai-consent">
          <label className="check">
            <input
              type="checkbox"
              checked={allowContent}
              onChange={(event) =>
                setAllowContent(event.currentTarget.checked)
              }
            />
            允许读取选中文件的文本内容（最多 30 个，每个约 6KB）
          </label>
          <div className="ai-settings-hint">
            支持文本、PDF、DOCX、PPTX、XLSX/XLS；图片、音视频、旧版 DOC/PPT 不会读取。内容只用于本次请求，AI 只返回文件名，不会修改文件。
          </div>
        </div>

        <div className="ai-generate-row">
          <button
            className="btn primary"
            disabled={!canGenerate}
            onClick={() => void generateContent(contentDraft)}
          >
            {generating ? <Loader className="spin" /> : <Sparkles />}
            {generating ? "生成中…" : "生成名字"}
          </button>
          {generating && (
            <button className="btn" onClick={() => void cancel()}>
              <Square />
              停止
            </button>
          )}
          <div className="spacer" />
          <span className="ai-provider">
            {active
              ? `${active.name}${active.model ? ` · ${active.model}` : " · 未选模型"}`
              : "未配置服务"}
          </span>
        </div>
      </div>

      {(generating || streamText) && (
        <div className="ai-stream">
          <div className="ai-stream-head">
            <Loader className="spin" />
            {status || "正在生成…"}
          </div>
          <pre>{streamText || "正在读取文件并等待模型输出…"}</pre>
        </div>
      )}

      {error && (
        <div className="ai-error">
          <AlertTriangle />
          <span>{error}</span>
          <button className="btn small" onClick={() => clearContent()}>
            知道了
          </button>
        </div>
      )}

      {contentResult && (
        <div className="ai-result">
          <div className="ai-result-head">
            <span className="ai-badge">
              {applied ? "已应用" : "名字已生成"}
            </span>
            <div className="spacer" />
            <span className="hint">
              {contentResult.entries.length} 个文件
            </span>
            {applied && <Check className="ai-ok" />}
          </div>

          <div className="ai-summary">{contentResult.summary}</div>

          <div className="ai-map-list">
            {contentResult.entries.map((entry) => (
              <div className="ai-map-row" key={entry.id}>
                <span className="ai-map-src" title={entry.srcName}>
                  {entry.srcName}
                </span>
                <span className="ai-map-arrow">→</span>
                <span
                  className="ai-map-dst"
                  title={withExt(entry.srcName, entry.stem)}
                >
                  <input
                    value={entry.stem}
                    spellCheck={false}
                    onChange={(event) =>
                      updateContentEntry(entry.id, event.currentTarget.value)
                    }
                  />
                  <span className="ai-map-ext">{extOf(entry.srcName)}</span>
                </span>
              </div>
            ))}
          </div>
          <div className="ai-settings-hint">
            新名字可以直接修改，改完再点「应用到预览」。
          </div>

          {contentResult.skipped.length > 0 && (
            <div className="ai-section">
              <span className="ai-section-label">未处理</span>
              <ul className="ai-list">
                {contentResult.skipped.slice(0, 8).map((item, index) => (
                  <li key={index}>
                    {item.name}：{item.reason}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {contentResult.assumptions.length > 0 && (
            <div className="ai-section">
              <span className="ai-section-label">假设</span>
              <ul className="ai-list">
                {contentResult.assumptions.map((item, index) => (
                  <li key={index}>{item}</li>
                ))}
              </ul>
            </div>
          )}

          {contentResult.unsupported.length > 0 && (
            <div className="ai-section ai-unsupported">
              <span className="ai-section-label">无法命名</span>
              <ul className="ai-list">
                {contentResult.unsupported.map((item, index) => (
                  <li key={index}>{item}</li>
                ))}
              </ul>
            </div>
          )}

          <div className="ai-actions">
            <button className="btn primary" onClick={applyContent}>
              <Check />
              应用到预览
            </button>
            <button
              className="btn"
              disabled={generating || !contentDraft.trim()}
              onClick={() => void generateContent(contentDraft)}
            >
              <RefreshCw />
              重新生成
            </button>
            <button className="btn ghost" onClick={() => clearContent()}>
              <X />
              移除方案
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
