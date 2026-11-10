import { Search, X } from "lucide-react";
import { useMemo, useState } from "react";

import type { RuleKind } from "../lib/types";
import { useApp } from "../store/appStore";
import { GROUPS, KIND_DESC, KIND_ICON, KIND_LABEL } from "./ruleCatalog";

export function RuleLibrary({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const addRule = useApp((s) => s.addRule);
  const [q, setQ] = useState("");
  const query = q.trim().toLowerCase();

  const groups = useMemo(() => {
    if (!query) return GROUPS;
    return GROUPS.map((g) => ({
      ...g,
      items: g.items.filter(
        (k) =>
          KIND_LABEL[k].toLowerCase().includes(query) ||
          KIND_DESC[k].toLowerCase().includes(query)
      ),
    })).filter((g) => g.items.length > 0);
  }, [query]);

  if (!open) return null;

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal"
        onClick={(e) => {
          // 点卡片内部不要传到遮罩上，否则一选规则就被关掉
          e.stopPropagation();
        }}
      >
        <div className="modal-head">
          <span className="panel-title">规则库</span>
          <span className="panel-count">15 条</span>
          <div className="spacer" />
          <button className="btn icon" title="关闭" onClick={onClose}>
            <X />
          </button>
        </div>

        <div className="search-box" style={{ margin: "10px 14px" }}>
          <Search />
          <input
            placeholder="搜索规则，如 截取 / 括号 / 时间"
            value={q}
            onChange={(e) => setQ(e.currentTarget.value)}
          />
        </div>

        <div className="modal-body">
          {groups.map((group) => (
            <div className="lib-group" key={group.title}>
              <div className="lib-title">{group.title}</div>
              {group.items.map((kind) => {
                const Icon = KIND_ICON[kind];
                return (
                  <button
                    key={kind}
                    className="lib-item"
                    onClick={() => addRule(kind as RuleKind)}
                    title={`添加一条「${KIND_LABEL[kind]}」`}
                  >
                    <span className="lib-icon">
                      <Icon />
                    </span>
                    <span className="lib-text">
                      <span className="lib-name">{KIND_LABEL[kind]}</span>
                      <span className="lib-desc">{KIND_DESC[kind]}</span>
                    </span>
                    <span className="lib-add">添加</span>
                  </button>
                );
              })}
            </div>
          ))}

          {groups.length === 0 && (
            <div className="empty" style={{ padding: "28px 8px" }}>
              <div className="big">没有匹配的规则</div>
              <div className="sub">换个词试试，比如「截取」「副本」「时间」</div>
            </div>
          )}
        </div>

        <div className="modal-foot">
          点「添加」会把规则放到栈尾，库不会关闭，可以连着挑几条再统一调参数。
        </div>
      </div>
    </div>
  );
}
