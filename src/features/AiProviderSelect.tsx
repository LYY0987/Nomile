import { useAi } from "../store/aiStore";

/** 快速切换已保存的 AI 服务，不进入设置弹层 */
export function AiProviderSelect({ disabled = false }: { disabled?: boolean }) {
  const providers = useAi((s) => s.providers);
  const activeId = useAi((s) => s.activeId);
  const setActive = useAi((s) => s.setActive);

  return (
    <select
      className="ai-provider-select"
      value={activeId}
      disabled={disabled || providers.length === 0}
      title="切换 AI 服务"
      onChange={(event) => void setActive(event.currentTarget.value)}
    >
      {providers.length === 0 ? (
        <option value="">未配置服务</option>
      ) : (
        providers.map((provider) => (
          <option key={provider.id} value={provider.id}>
            {provider.name}
            {provider.model ? ` · ${provider.model}` : " · 未选模型"}
          </option>
        ))
      )}
    </select>
  );
}
