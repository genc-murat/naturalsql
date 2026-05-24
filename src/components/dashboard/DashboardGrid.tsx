import { useState, useEffect } from "react";
import {
  X,
  Plus,
  Trash2,
  RefreshCw,
  Layout,
  Maximize2,
  Minimize2,
  AlertCircle,
  Loader2,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import type { Dashboard, DashboardWidget, QueryResult } from "../../types";
import { AutoChart } from "../AutoChart";
import { ResultsTable } from "../ResultsTable";

interface DashboardGridProps {
  dashboard: Dashboard;
  onClose: () => void;
  onRefresh?: () => void;
}

export function DashboardGrid({ dashboard, onClose }: DashboardGridProps) {
  const [widgets, setWidgets] = useState<DashboardWidget[]>(dashboard.widgets);
  const [refreshing, setRefreshing] = useState<Record<string, boolean>>({});

  const refreshWidget = async (widgetId: string) => {
    const widget = widgets.find((w) => w.id === widgetId);
    if (!widget) return;

    setRefreshing((prev) => ({ ...prev, [widgetId]: true }));
    try {
      const result: QueryResult = await invoke("execute_query", { sql: widget.sql });
      setWidgets((prev) =>
        prev.map((w) => (w.id === widgetId ? { ...w, result, error: null } : w))
      );
    } catch (err) {
      setWidgets((prev) =>
        prev.map((w) => (w.id === widgetId ? { ...w, error: String(err) } : w))
      );
    } finally {
      setRefreshing((prev) => ({ ...prev, [widgetId]: false }));
    }
  };

  useEffect(() => {
    // Initial load for widgets without results
    widgets.forEach((w) => {
      if (!w.result && !w.error) {
        refreshWidget(w.id);
      }
    });
  }, []);

  return (
    <div className="fixed inset-0 z-50 bg-[var(--bg-primary)] flex flex-col">
      <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-lg bg-[var(--accent)]/10 text-[var(--accent)]">
            <Layout className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-lg font-bold text-[var(--text-primary)]">{dashboard.title}</h2>
            {dashboard.description && (
              <p className="text-xs text-[var(--text-muted)]">{dashboard.description}</p>
            )}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => widgets.forEach((w) => refreshWidget(w.id))}
            className="p-2 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-secondary)] transition-colors"
            title="Refresh All"
          >
            <RefreshCw className="w-5 h-5" />
          </button>
          <button
            onClick={onClose}
            className="p-2 rounded-md hover:bg-red-500/10 hover:text-red-500 text-[var(--text-muted)] transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-auto p-6 bg-[var(--bg-primary)]">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {widgets.map((widget) => (
            <div
              key={widget.id}
              className={`flex flex-col rounded-xl border border-[var(--border)] bg-[var(--bg-secondary)] shadow-sm transition-all hover:shadow-md ${
                widget.type === "table" ? "lg:col-span-2 row-span-2" : ""
              }`}
            >
              <div className="flex items-center justify-between px-4 py-3 border-b border-[var(--border)]">
                <h3 className="text-sm font-semibold text-[var(--text-secondary)] truncate">
                  {widget.title}
                </h3>
                <div className="flex items-center gap-1">
                  {refreshing[widget.id] && (
                    <Loader2 className="w-3.5 h-3.5 animate-spin text-[var(--accent)]" />
                  )}
                  <button
                    onClick={() => refreshWidget(widget.id)}
                    className="p-1 rounded hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)]"
                  >
                    <RefreshCw className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>

              <div className="flex-1 min-h-[300px] p-4 relative overflow-hidden">
                {widget.error ? (
                  <div className="flex flex-col items-center justify-center h-full text-center p-4">
                    <AlertCircle className="w-8 h-8 text-red-500/50 mb-2" />
                    <p className="text-xs text-red-400 max-w-[200px] break-words">{widget.error}</p>
                  </div>
                ) : !widget.result ? (
                  <div className="flex items-center justify-center h-full">
                    <Loader2 className="w-8 h-8 animate-spin text-[var(--accent)]/20" />
                  </div>
                ) : (
                  <WidgetContent widget={widget} />
                )}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function WidgetContent({ widget }: { widget: DashboardWidget }) {
  if (!widget.result) return null;

  if (widget.type === "stat") {
    const val = widget.result.rows[0]?.[0];
    return (
      <div className="flex flex-col items-center justify-center h-full">
        <span className="text-4xl font-bold text-[var(--text-primary)] tracking-tight">
          {typeof val === "number" ? val.toLocaleString() : String(val ?? "0")}
        </span>
        <span className="text-xs text-[var(--text-muted)] mt-2 uppercase tracking-wider font-medium">
          {widget.result.columns[0]}
        </span>
      </div>
    );
  }

  if (widget.type === "table") {
    return (
      <div className="h-full overflow-auto">
        <ResultsTable result={widget.result} hideHeader dense />
      </div>
    );
  }

  return (
    <div className="h-full">
      <AutoChart columns={widget.result.columns} rows={widget.result.rows} />
    </div>
  );
}
