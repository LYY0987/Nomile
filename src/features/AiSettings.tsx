import { Loader, Plus, Settings, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";

import type { AiProviderConfig, AiProviderView } from "../lib/types";
import { useAi } from "../store/aiStore";
import { AI_PRESETS } from "./aiPresets";

export function AiSettings({ onClose }: { onClose: () => void }) {
  const providers = useAi((s) => s.providers);
  const activeId = useAi((s) => s.activeId);
  const loadProviders = useAi((s) => s.loadProviders);
  const saveProvider = useAi((s) => s.saveProvider);
  const deleteProvider = useAi((s) => s.deleteProvider);
  const setActive = useAi((s) => s.setActive);
  const setKey = useAi((s) => s.setKey);
  const deleteKey = useAi((s) => s.deleteKey);
  const loadModels = useAi((s) => s.loadModels);
  const testProvider = useAi((s) => s.testProvider);

  const [form, setForm] = useState<AiProviderConfig>({
    id: "",
    name: "",
    kind: "ollama",
    baseUrl: "http://localhost:11434",
    model: "",
  });
  const [presetId, setPresetId] = useState("ollama");
  const [apiKey, setApiKey] = useState("");
  const [modelOptions, setModelOptions] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");

  const isSaved = providers.some((p) => p.id === form.id);
  const current = providers.find((p) => p.id === form.id);

  useEffect(() => {
    void loadProviders();
  }, [loadProviders]);

  useEffect(() => {
    if (form.id) return;
    const provider = providers.find((p) => p.id === activeId) ?? providers[0];
    if (provider) selectProvider(provider);
    // 只在还没选中服务时自动落到默认服务
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [providers, activeId]);

  function selectProvider(provider: AiProviderView) {
    setForm({
      id: provider.id,
      name: provider.name,
      kind: provider.kind,
      baseUrl: provider.baseUrl,
      model: provider.model,
    });
    setPresetId(AI_PRESETS.some((p) => p.id === provider.id) ? provider.id : "custom");
    setApiKey("");
    setModelOptions([]);
    setMessage("");
  }

  function applyPreset(id: string) {
    setPresetId(id);
    const preset = AI_PRESETS.find((p) => p.id === id);
    if (!preset) return;
    setForm((f) => ({
      ...f,
      name: preset.name,
      kind: preset.kind,
      baseUrl: preset.baseUrl,
      model: preset.model,
    }));
  }

  function newProvider() {
    const preset = AI_PRESETS.find((p) => p.id === presetId) ?? AI_PRESETS[0];
    setForm({
      id: `p${Date.now().toString(36)}`,
      name: preset.name,
      kind: preset.kind,
      baseUrl: preset.baseUrl,
      model: preset.model,
    });
    setApiKey("");
    setModelOptions([]);
    setMessage("新服务：填好后点保存");
  }

  async function save() {
    setBusy(true);
    const ok = await saveProvider(form);
    setBusy(false);
    if (ok) setMessage("已保存");
  }

  async function remove() {
    if (!isSaved) return;
    if (!window.confirm(`删除服务「${form.name}」？`)) return;
    await deleteProvider(form.id);
    setForm({
      id: "",
      name: "",
      kind: "ollama",
      baseUrl: "http://localhost:11434",
      model: "",
    });
    setApiKey("");
    setModelOptions([]);
    setMessage("已删除");
  }

  async function saveKey() {
    if (!isSaved) {
      setMessage("先保存服务，再设置密钥");
      return;
    }
    if (!apiKey.trim()) return;
    const ok = await setKey(form.id, apiKey);
    if (ok) {
      setApiKey("");
      setMessage("密钥已保存到系统钥匙串");
    }
  }

  async function removeKey() {
    if (!isSaved) return;
    await deleteKey(form.id);
    setMessage("已删除密钥");
  }

  async function fetchModels() {
    if (!isSaved) {
      setMessage("先保存服务，再拉取模型");
      return;
    }
    setBusy(true);
    const models = await loadModels(form.id);
    setBusy(false);
    setModelOptions(models);
    if (models.length === 0) {
      setMessage("没有拉到模型，请检查接口地址和密钥");
    } else {
      setMessage(`拉到 ${models.length} 个模型`);
      if (!form.model) {
        setForm((f) => ({ ...f, model: models[0] }));
      }
    }
  }

  async function test() {
    if (!isSaved) {
      setMessage("先保存服务，再测试连接");
      return;
    }
    setBusy(true);
    const result = await testProvider(form.id);
    setBusy(false);
    if (result) setMessage(result);
  }

  async function makeDefault() {
    if (!isSaved) return;
    await setActive(form.id);
    setMessage("已设为默认服务");
  }

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal ai-settings-modal"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="modal-head">
          <Settings />
          <span className="panel-title">AI 服务设置</span>
          <div className="spacer" />
          <button className="btn icon" title="关闭" onClick={onClose}>
            <X />
          </button>
        </div>

        <div className="ai-settings-body">
          <aside className="ai-provider-list">
            <div className="lib-title">已保存服务</div>
            {providers.map((provider) => (
              <button
                key={provider.id}
                className={`ai-provider-item${provider.id === form.id ? " is-active" : ""}`}
                onClick={() => selectProvider(provider)}
              >
                <span className="ai-provider-name">{provider.name}</span>
                <span className="ai-provider-meta">
                  {provider.model || "未选模型"}
                  {provider.id === activeId ? " · 默认" : ""}
                  {provider.hasKey ? " · 有密钥" : ""}
                </span>
              </button>
            ))}
            <button className="btn small" onClick={newProvider}>
              <Plus />
              新增服务
            </button>
          </aside>

          <div className="ai-settings-form">
            <div className="field">
              <span className="label">预设</span>
              <select
                className="input small"
                value={presetId}
                onChange={(event) => applyPreset(event.currentTarget.value)}
              >
                {AI_PRESETS.map((preset) => (
                  <option key={preset.id} value={preset.id}>
                    {preset.name}
                  </option>
                ))}
              </select>
            </div>

            <div className="field">
              <span className="label">名称</span>
              <input
                className="input small"
                value={form.name}
                onChange={(event) =>
                  setForm((f) => ({ ...f, name: event.currentTarget.value }))
                }
              />
            </div>

            <div className="field">
              <span className="label">类型</span>
              <select
                className="input small"
                value={form.kind}
                onChange={(event) =>
                  setForm((f) => ({
                    ...f,
                    kind: event.currentTarget.value as AiProviderConfig["kind"],
                  }))
                }
              >
                <option value="ollama">Ollama</option>
                <option value="openai">OpenAI 兼容</option>
              </select>
            </div>

            <div className="field">
              <span className="label">接口地址</span>
              <input
                className="input small mono"
                value={form.baseUrl}
                placeholder="https://api.example.com/v1"
                onChange={(event) =>
                  setForm((f) => ({ ...f, baseUrl: event.currentTarget.value }))
                }
              />
            </div>

            <div className="field">
              <span className="label">模型</span>
              <input
                className="input small mono"
                value={form.model}
                placeholder="如 qwen2.5:7b"
                onChange={(event) =>
                  setForm((f) => ({ ...f, model: event.currentTarget.value }))
                }
              />
              <button
                className="btn small"
                style={{ height: 26 }}
                disabled={busy}
                onClick={() => void fetchModels()}
              >
                {busy ? <Loader className="spin" /> : null}
                拉取
              </button>
            </div>

            {modelOptions.length > 0 && (
              <div className="field">
                <span className="label">可选</span>
                <select
                  className="input small"
                  value={form.model}
                  onChange={(event) =>
                    setForm((f) => ({ ...f, model: event.currentTarget.value }))
                  }
                >
                  <option value="">选择模型</option>
                  {modelOptions.map((model) => (
                    <option key={model} value={model}>
                      {model}
                    </option>
                  ))}
                </select>
              </div>
            )}

            <div className="field">
              <span className="label">API Key</span>
              <input
                className="input small mono"
                type="password"
                value={apiKey}
                placeholder={current?.hasKey ? "已保存，留空不修改" : "粘贴密钥"}
                onChange={(event) => setApiKey(event.currentTarget.value)}
              />
            </div>

            <div className="ai-settings-actions">
              <button className="btn small" disabled={busy} onClick={() => void saveKey()}>
                保存密钥
              </button>
              <button
                className="btn small"
                disabled={busy || !current?.hasKey}
                onClick={() => void removeKey()}
              >
                删除密钥
              </button>
              <div className="spacer" />
              <button className="btn small" disabled={busy} onClick={() => void test()}>
                测试连接
              </button>
            </div>

            <div className="ai-settings-actions">
              <button className="btn primary" disabled={busy} onClick={() => void save()}>
                {busy ? <Loader className="spin" /> : null}
                保存服务
              </button>
              <button
                className="btn small"
                disabled={!isSaved || form.id === activeId}
                onClick={() => void makeDefault()}
              >
                设为默认
              </button>
              <div className="spacer" />
              <button
                className="btn small danger"
                disabled={!isSaved}
                onClick={() => void remove()}
              >
                <Trash2 />
                删除
              </button>
            </div>

            {message && <div className="ai-settings-msg">{message}</div>}
            <div className="ai-settings-hint">
              密钥只保存在系统钥匙串，不写入配置文件；默认只上传文件名样本，不读取文件内容。
            </div>
          </div>
        </div>

        <div className="modal-foot">
          支持本地 Ollama 与 OpenAI 兼容接口。选好模型后回到 AI 面板即可生成方案。
        </div>
      </div>
    </div>
  );
}
