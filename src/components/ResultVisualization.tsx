import { useState, useMemo } from "react";
import {
  Table2,
  BarChart3,
  TrendingUp,
  Clock,
  Rows3,
} from "lucide-react";
import type { QueryResult } from "../types";
import { ResultsTable } from "./ResultsTable";
import { AutoChart } from "./AutoChart";
import { DataStatistics } from "./DataStatistics";
import { ResultActions } from "./ResultActions";

interface ResultVisualizationProps {
  result: QueryResult | null;
  onApplySql?: (sql: string) => void;
}

type TabId = "table" | "charts" | "stats";

interface TabDef {
  id: TabId;
  label: string;
  icon: React.ReactNode;
}

const TABS: TabDef[] = [
  { id: "table", label: "Table", icon: <Table2 className="w-4 h-4" /> },
  { id: "charts", label: "Charts", icon: <BarChart3 className="w-4 h-4" /> },
  { id: "stats", label: "Statistics", icon: <TrendingUp className="w-4 h-4" /> },
];

export function ResultVisualization({ result, onApplySql }: ResultVisualizationProps) {
  const [activeTab, setActiveTab] = useState<TabId>("table");

  const hasData = result && result.columns.length > 0;
  const isWrite = result?.affected_rows !== null && result?.affected_rows !== undefined;

  const numericColCount = useMemo(() => {
    if (!result) return 0;
    return result.columns.filter((_, i) => {
      const sample = result.rows.slice(0, 10).map((r) => r[i]);
      return sample.filter((v) => typeof v === "number" || (typeof v === "string" && !isNaN(Number(v)))).length > sample.length * 0.5;
    }).length;
  }, [result]);

  if (!hasData) {
    return (
      <div className="flex items-center justify-center h-full text-[var(--text-muted)]">
        <div className="text-center">
          <Rows3 className="w-12 h-12 mx-auto mb-4 opacity-20" />
          <p className="text-base font-medium mb-1">No results yet</p>
          <p className="text-sm">Write a query or use natural language to get started</p>
        </div>
      </div>
    );
  }

  const execTime = result.execution_time_ms;
  const timeStr = execTime < 1000 ? `${execTime}ms` : `${(execTime / 1000).toFixed(1)}s`;

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-4 py-2 border-b border-[var(--border)] bg-[var(--bg-secondary)] shrink-0">
        <div className="flex items-center gap-1">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${
                activeTab === tab.id
                  ? "bg-[var(--accent)]/10 text-[var(--accent)]"
                  : "text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[var(--bg-tertiary)]"
              }`}
            >
              {tab.icon}
              {tab.label}
              {tab.id === "charts" && numericColCount > 0 && (
                <span className="ml-0.5 px-1.5 py-0 rounded-full bg-[var(--accent)]/20 text-[10px] text-[var(--accent)]">
                  {numericColCount}n
                </span>
              )}
            </button>
          ))}
        </div>

        <div className="flex items-center gap-3">
          <span className="text-sm text-[var(--text-secondary)]">
            {isWrite
              ? `${result.affected_rows} row${result.affected_rows !== 1 ? "s" : ""} affected`
              : `${result.row_count} row${result.row_count !== 1 ? "s" : ""}`}
          </span>
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-[var(--bg-tertiary)] border border-[var(--border)] text-xs text-[var(--text-muted)]">
            <Clock className="w-3 h-3" />
            {timeStr}
          </span>
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-auto p-4">
        {activeTab === "table" && (
          <div className="space-y-3">
            <ResultsTable result={result} onApplySql={onApplySql} hideHeader />
            <ResultActions
              columns={result.columns}
              rows={result.rows}
              rowCount={result.row_count}
              onApplySql={onApplySql || (() => {})}
            />
          </div>
        )}
        {activeTab === "charts" && (
          <AutoChart columns={result.columns} rows={result.rows} />
        )}
        {activeTab === "stats" && (
          <DataStatistics columns={result.columns} rows={result.rows} />
        )}
      </div>
    </div>
  );
}
