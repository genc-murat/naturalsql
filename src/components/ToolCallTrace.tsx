import { useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Search,
  Table2,
  Database,
  GitBranch,
  Shield,
  Server,
  BarChart3,
  FileText,
  Lightbulb,
  AlertTriangle,
  CheckCircle,
} from "lucide-react";
import type { ToolCallStep } from "../types";

interface ToolCallTraceProps {
  steps: ToolCallStep[];
  iterations: number;
  usedFallback: boolean;
}

const TOOL_ICONS: Record<string, React.ReactNode> = {
  list_tables: <Database className="w-3.5 h-3.5" />,
  get_table_schema: <Table2 className="w-3.5 h-3.5" />,
  get_sample_data: <BarChart3 className="w-3.5 h-3.5" />,
  get_indexes: <Search className="w-3.5 h-3.5" />,
  get_foreign_keys: <GitBranch className="w-3.5 h-3.5" />,
  get_constraints: <FileText className="w-3.5 h-3.5" />,
  list_views: <FileText className="w-3.5 h-3.5" />,
  list_procedures: <FileText className="w-3.5 h-3.5" />,
  list_triggers: <FileText className="w-3.5 h-3.5" />,
  get_table_stats: <BarChart3 className="w-3.5 h-3.5" />,
  get_table_status: <Server className="w-3.5 h-3.5" />,
  find_relationships: <GitBranch className="w-3.5 h-3.5" />,
  find_similar_columns: <Search className="w-3.5 h-3.5" />,
  compare_tables: <GitBranch className="w-3.5 h-3.5" />,
  cross_db_join: <GitBranch className="w-3.5 h-3.5" />,
  explain_query: <Lightbulb className="w-3.5 h-3.5" />,
  security_check: <Shield className="w-3.5 h-3.5" />,
  validate_sql: <CheckCircle className="w-3.5 h-3.5" />,
  get_server_info: <Server className="w-3.5 h-3.5" />,
};

const TOOL_CATEGORIES: Record<string, string> = {
  list_tables: "schema",
  get_table_schema: "schema",
  get_sample_data: "schema",
  get_indexes: "schema",
  get_foreign_keys: "schema",
  get_constraints: "schema",
  list_views: "schema",
  list_procedures: "schema",
  list_triggers: "schema",
  get_table_stats: "schema",
  get_table_status: "schema",
  find_relationships: "relationship",
  find_similar_columns: "relationship",
  compare_tables: "relationship",
  cross_db_join: "relationship",
  explain_query: "analysis",
  security_check: "analysis",
  validate_sql: "analysis",
  get_server_info: "server",
};

const CATEGORY_STYLES: Record<string, { bg: string; text: string; border: string; badge: string }> = {
  schema: {
    bg: "bg-blue-500/5",
    text: "text-blue-400",
    border: "border-blue-500/20",
    badge: "bg-blue-500/10 text-blue-400",
  },
  relationship: {
    bg: "bg-purple-500/5",
    text: "text-purple-400",
    border: "border-purple-500/20",
    badge: "bg-purple-500/10 text-purple-400",
  },
  analysis: {
    bg: "bg-green-500/5",
    text: "text-green-400",
    border: "border-green-500/20",
    badge: "bg-green-500/10 text-green-400",
  },
  server: {
    bg: "bg-gray-500/5",
    text: "text-gray-400",
    border: "border-gray-500/20",
    badge: "bg-gray-500/10 text-gray-400",
  },
};

function getToolDisplayName(name: string): string {
  return name
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

export function ToolCallTrace({ steps, iterations, usedFallback }: ToolCallTraceProps) {
  const [expanded, setExpanded] = useState(false);
  const [expandedSteps, setExpandedSteps] = useState<Set<number>>(new Set());

  if (steps.length === 0) {
    if (!usedFallback) return null;
    return (
      <div className="px-3 py-2 rounded-lg bg-amber-500/5 border border-amber-500/20">
        <div className="flex items-center gap-2">
          <AlertTriangle className="w-3.5 h-3.5 text-amber-400" />
          <span className="text-xs text-amber-400">
            Used fallback (schema too large for tool-based exploration)
          </span>
        </div>
      </div>
    );
  }

  const toggleStep = (idx: number) => {
    setExpandedSteps((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  };

  return (
    <div className="rounded-lg border border-[var(--border)] overflow-hidden">
      {/* Header */}
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center justify-between px-3 py-2 bg-[var(--bg-secondary)] hover:bg-[var(--bg-tertiary)] transition-colors"
      >
        <div className="flex items-center gap-2">
          {expanded ? (
            <ChevronDown className="w-3.5 h-3.5 text-[var(--text-muted)]" />
          ) : (
            <ChevronRight className="w-3.5 h-3.5 text-[var(--text-muted)]" />
          )}
          <span className="text-xs font-medium text-[var(--text-secondary)]">
            Tool Calls
          </span>
          <span className="px-1.5 py-0.5 rounded-md bg-[var(--accent)]/10 text-[var(--accent)] text-[10px] font-semibold">
            {steps.length}
          </span>
          <span className="text-[10px] text-[var(--text-muted)]">
            in {iterations} iteration{iterations !== 1 ? "s" : ""}
          </span>
        </div>
      </button>

      {/* Steps */}
      {expanded && (
        <div className="divide-y divide-[var(--border)] bg-[var(--bg-primary)]">
          {steps.map((step, idx) => {
            const category = TOOL_CATEGORIES[step.tool_name] || "schema";
            const style = CATEGORY_STYLES[category] || CATEGORY_STYLES.schema;
            const icon = TOOL_ICONS[step.tool_name] || <Search className="w-3.5 h-3.5" />;
            const isExpanded = expandedSteps.has(idx);

            return (
              <div key={idx} className={`${style.bg}`}>
                <button
                  onClick={() => toggleStep(idx)}
                  className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[var(--bg-tertiary)]/50 transition-colors"
                >
                  <span className={`${style.text}`}>{icon}</span>
                  <span className="text-xs text-[var(--text-secondary)] font-medium">
                    {getToolDisplayName(step.tool_name)}
                  </span>
                  <span className={`px-1 py-0.5 rounded text-[10px] ${style.badge}`}>
                    iter {step.iteration}
                  </span>
                  {isExpanded ? (
                    <ChevronDown className="w-3 h-3 text-[var(--text-muted)] ml-auto" />
                  ) : (
                    <ChevronRight className="w-3 h-3 text-[var(--text-muted)] ml-auto" />
                  )}
                </button>

                {isExpanded && (
                  <div className="px-3 pb-2">
                    {/* Parameters */}
                    {Object.keys(step.parameters).length > 0 && (
                      <div className="mb-1.5">
                        <span className="text-[10px] text-[var(--text-muted)] font-medium">
                          Parameters:
                        </span>
                        <div className="flex flex-wrap gap-1 mt-1">
                          {Object.entries(step.parameters).map(([key, value]) => (
                            <span
                              key={key}
                              className="px-1.5 py-0.5 rounded bg-[var(--bg-tertiary)] border border-[var(--border)] text-[10px] text-[var(--text-secondary)] font-mono"
                            >
                              {key}: {value}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}

                    {/* Result */}
                    <div>
                      <span className="text-[10px] text-[var(--text-muted)] font-medium">
                        Result:
                      </span>
                      <pre className="mt-1 p-2 rounded-md bg-[var(--bg-secondary)] border border-[var(--border)] text-[11px] text-[var(--text-secondary)] font-mono whitespace-pre-wrap break-all max-h-32 overflow-y-auto">
                        {step.result}
                      </pre>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
