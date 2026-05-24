import { useState } from "react";
import { Plus, X, FileText } from "lucide-react";
import type { QueryTab } from "../types";

interface QueryTabsProps {
  tabs: QueryTab[];
  activeTabId: string;
  onSelectTab: (id: string) => void;
  onCloseTab: (id: string) => void;
  onNewTab: () => void;
  onRenameTab: (id: string, name: string) => void;
}

export function QueryTabs({
  tabs,
  activeTabId,
  onSelectTab,
  onCloseTab,
  onNewTab,
  onRenameTab,
}: QueryTabsProps) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");

  const handleDoubleClick = (tab: QueryTab) => {
    setEditingId(tab.id);
    setEditValue(tab.name);
  };

  const handleFinishEdit = () => {
    if (editingId && editValue.trim()) {
      onRenameTab(editingId, editValue.trim());
    }
    setEditingId(null);
  };

  return (
    <div className="flex items-center border-b border-[var(--border)] bg-[var(--bg-secondary)] overflow-x-auto shrink-0">
      {tabs.map((tab) => {
        const isActive = tab.id === activeTabId;
        return (
          <div
            key={tab.id}
            className={`group flex items-center gap-1.5 px-3 py-1.5 cursor-pointer border-r border-[var(--border)] min-w-0 max-w-[180px] transition-colors ${
              isActive
                ? "bg-[var(--bg-primary)] text-[var(--text-primary)]"
                : "text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)] hover:text-[var(--text-secondary)]"
            }`}
            onClick={() => onSelectTab(tab.id)}
            onDoubleClick={() => handleDoubleClick(tab)}
          >
            <FileText className="w-3.5 h-3.5 flex-shrink-0 opacity-60" />
            {editingId === tab.id ? (
              <input
                type="text"
                value={editValue}
                onChange={(e) => setEditValue(e.target.value)}
                onBlur={handleFinishEdit}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleFinishEdit();
                  if (e.key === "Escape") setEditingId(null);
                }}
                autoFocus
                className="flex-1 min-w-0 px-1 py-0 bg-[var(--bg-tertiary)] border border-[var(--accent)] rounded text-xs text-[var(--text-primary)] focus:outline-none"
                onClick={(e) => e.stopPropagation()}
              />
            ) : (
              <span className="truncate text-xs font-medium">{tab.name}</span>
            )}
            {tabs.length > 1 && (
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onCloseTab(tab.id);
                }}
                className="p-0.5 rounded opacity-0 group-hover:opacity-100 hover:bg-[var(--bg-tertiary)] transition-opacity flex-shrink-0"
              >
                <X className="w-3 h-3" />
              </button>
            )}
          </div>
        );
      })}
      <button
        onClick={onNewTab}
        className="flex items-center px-2 py-1.5 text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[var(--bg-tertiary)] transition-colors flex-shrink-0"
        title="New tab (Ctrl+T)"
      >
        <Plus className="w-4 h-4" />
      </button>
    </div>
  );
}
