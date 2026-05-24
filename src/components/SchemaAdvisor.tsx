import { useState, useCallback } from "react";
import {
  X,
  Loader2,
  AlertTriangle,
  AlertCircle,
  Info,
  HardDrive,
  Database,
  Table2,
  Lightbulb,
  ChevronDown,
  ChevronRight,
  Search,
  RefreshCw,
  Link,
  Key,
} from "lucide-react";
import { schemaAdvisor } from "../api";
import type { AdvisorIssue, AdvisorResponse } from "../types";

interface SchemaAdvisorProps {
  isOpen: boolean;
  onClose: () => void;
  database: string;
  cachedDatabases: string[];
}

const severityConfig = {
  critical: {
    icon: AlertCircle,
    color: "text-red-500",
    bg: "bg-red-500/10",
    border: "border-red-500/20",
    badge: "bg-red-500/20 text-red-400",
    label: "Critical",
  },
  warning: {
    icon: AlertTriangle,
    color: "text-amber-500",
    bg: "bg-amber-500/10",
    border: "border-amber-500/20",
    badge: "bg-amber-500/20 text-amber-400",
    label: "Warning",
  },
  info: {
    icon: Info,
    color: "text-blue-500",
    bg: "bg-blue-500/10",
    border: "border-blue-500/20",
    badge: "bg-blue-500/20 text-blue-400",
    label: "Info",
  },
};

const categoryColors: Record<string, string> = {
  INDEX: "text-purple-500",
  PERFORMANCE: "text-orange-500",
  NORMALIZATION: "text-teal-500",
  DATA_QUALITY: "text-pink-500",
  SCHEMA_DESIGN: "text-yellow-500",
  SECURITY: "text-red-500",
  STORAGE: "text-cyan-500",
};

function IssueCard({ issue, defaultExpanded }: { issue: AdvisorIssue; defaultExpanded?: boolean }) {
  const [expanded, setExpanded] = useState(defaultExpanded || false);
  const cfg = severityConfig[issue.severity] || severityConfig.info;
  const Icon = cfg.icon;
  const categoryColor = categoryColors[issue.category] || "text-[var(--text-muted)]";

  return (
    <div
      className={`rounded-lg border ${cfg.border} ${cfg.bg}/30 overflow-hidden transition-all duration-200 hover:shadow-sm`}
    >
      {/* Header - clickable to expand */}
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-start gap-3 px-4 py-3 text-left transition-colors hover:bg-black/5 dark:hover:bg-white/5"
      >
        <div className={`mt-0.5 p-1 rounded-full ${cfg.bg}`}>
          <Icon className={`w-4 h-4 ${cfg.color}`} />
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-sm font-medium text-[var(--text-primary)]">{issue.title}</span>
            <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${cfg.badge}`}>
              {cfg.label}
            </span>
            <span className={`text-[10px] font-mono uppercase tracking-wider ${categoryColor}`}>
              {issue.category}
            </span>
          </div>
          <p className="text-xs text-[var(--text-muted)] mt-0.5 line-clamp-1">{issue.description}</p>
        </div>
        <div className="shrink-0 mt-1">
          {expanded ? (
            <ChevronDown className="w-4 h-4 text-[var(--text-muted)]" />
          ) : (
            <ChevronRight className="w-4 h-4 text-[var(--text-muted)]" />
          )}
        </div>
      </button>

      {/* Expanded content */}
      {expanded && (
        <div className="px-4 pb-4 pt-1 border-t border-[var(--border)]/50">
          <div className="space-y-2.5 text-sm">
            <div>
              <span className="text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Description</span>
              <p className="text-[var(--text-secondary)] mt-0.5">{issue.description}</p>
            </div>
            <div>
              <span className="text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Suggestion</span>
              <div className="flex items-start gap-2 mt-0.5">
                <Lightbulb className="w-4 h-4 text-amber-400 mt-0.5 shrink-0" />
                <p className="text-[var(--text-primary)]">{issue.suggestion}</p>
              </div>
            </div>
            {(issue.table || issue.column) && (
              <div className="flex items-center gap-4 pt-1">
                {issue.table && (
                  <div className="flex items-center gap-1.5 text-xs text-[var(--text-muted)]">
                    <Table2 className="w-3.5 h-3.5" />
                    <span className="font-mono">{issue.table}</span>
                  </div>
                )}
                {issue.column && (
                  <div className="flex items-center gap-1.5 text-xs text-[var(--text-muted)]">
                    <Key className="w-3.5 h-3.5" />
                    <span className="font-mono">{issue.column}</span>
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export function SchemaAdvisor({ isOpen, onClose, database, cachedDatabases }: SchemaAdvisorProps) {
  const [result, setResult] = useState<AdvisorResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const [filterSeverity, setFilterSeverity] = useState<string | null>(null);
  const [filterCategory, setFilterCategory] = useState<string | null>(null);

  const isCached = cachedDatabases.includes(database);

  const runAnalysis = useCallback(async () => {
    if (!database) {
      setError("No database selected. Please select a database first.");
      return;
    }
    setLoading(true);
    setError("");
    setResult(null);
    setSearchQuery("");
    setFilterSeverity(null);
    setFilterCategory(null);
    try {
      const res = await schemaAdvisor(database);
      setResult(res);
      if (res.issues.length === 0) {
        setError("No issues found. Your database looks healthy!");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [database]);

  if (!isOpen) return null;

  const filteredIssues = result?.issues.filter((issue) => {
    if (filterSeverity && issue.severity !== filterSeverity) return false;
    if (filterCategory && issue.category !== filterCategory) return false;
    if (searchQuery) {
      const q = searchQuery.toLowerCase();
      return (
        issue.title.toLowerCase().includes(q) ||
        issue.description.toLowerCase().includes(q) ||
        issue.suggestion.toLowerCase().includes(q) ||
        issue.table?.toLowerCase().includes(q) ||
        issue.column?.toLowerCase().includes(q)
      );
    }
    return true;
  }) || [];

  // Group filtered issues by severity
  const groupedBySeverity = {
    critical: filteredIssues.filter((i) => i.severity === "critical"),
    warning: filteredIssues.filter((i) => i.severity === "warning"),
    info: filteredIssues.filter((i) => i.severity === "info"),
  };

  const categories = result ? [...new Set(result.issues.map((i) => i.category))] : [];
  const severityCounts = result ? {
    critical: result.issues.filter((i) => i.severity === "critical").length,
    warning: result.issues.filter((i) => i.severity === "warning").length,
    info: result.issues.filter((i) => i.severity === "info").length,
  } : { critical: 0, warning: 0, info: 0 };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="w-[820px] max-h-[90vh] bg-[var(--bg-primary)] border border-[var(--border)] rounded-xl shadow-2xl flex flex-col overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
          <div className="flex items-center gap-3">
            <Search className="w-5 h-5 text-[var(--accent)]" />
            <div>
              <h2 className="text-sm font-semibold text-[var(--text-primary)]">
                Schema Advisor
              </h2>
              <p className="text-xs text-[var(--text-muted)] mt-0.5">
                AI-powered database analysis & optimization suggestions
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] transition-colors"
          >
            <X className="w-4 h-4 text-[var(--text-muted)]" />
          </button>
        </div>

        {/* Initial state: Run Analysis prompt */}
        {!result && !loading && !error && (
          <div className="flex flex-col items-center justify-center py-20 px-6">
            <div className="p-4 rounded-full bg-[var(--accent)]/10 mb-4">
              <Search className="w-8 h-8 text-[var(--accent)]" />
            </div>
            <h3 className="text-lg font-semibold text-[var(--text-primary)] mb-2">
              Analyze Database Health
            </h3>
            <p className="text-sm text-[var(--text-muted)] text-center max-w-md mb-6">
              Get AI-powered insights on missing indexes, data quality issues,
              normalization opportunities, and performance bottlenecks for{" "}
              <span className="font-mono text-[var(--accent)]">{database}</span>.
            </p>
            {!isCached && (
              <p className="text-xs text-amber-400 mb-4 flex items-center gap-1.5">
                <AlertTriangle className="w-3.5 h-3.5" />
                This database is not cached. Please cache it first in the sidebar.
              </p>
            )}
            <button
              onClick={runAnalysis}
              disabled={loading || !database}
              className="px-6 py-2.5 rounded-lg bg-[var(--accent)] text-white text-sm font-medium hover:opacity-90 transition-all disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2 shadow-lg shadow-[var(--accent)]/20"
            >
              {loading ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  Analyzing...
                </>
              ) : (
                <>
                  <RefreshCw className="w-4 h-4" />
                  Run Analysis
                </>
              )}
            </button>
          </div>
        )}

        {/* Loading state */}
        {loading && (
          <div className="flex flex-col items-center justify-center py-20">
            <Loader2 className="w-8 h-8 animate-spin text-[var(--accent)] mb-4" />
            <p className="text-sm text-[var(--text-muted)]">Analyzing database structure...</p>
            <p className="text-xs text-[var(--text-muted)] mt-1">Collecting statistics, checking indexes, querying LLM...</p>
          </div>
        )}

        {/* Error state */}
        {error && !loading && !result && (
          <div className="py-12 px-6 text-center">
            <AlertCircle className="w-10 h-10 text-[var(--error)] mx-auto mb-3" />
            <p className="text-sm text-[var(--error)] mb-4">{error}</p>
            <button
              onClick={runAnalysis}
              className="px-4 py-2 rounded-lg bg-[var(--accent)] text-white text-sm font-medium hover:opacity-90 transition-all flex items-center gap-2 mx-auto"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              Try Again
            </button>
          </div>
        )}

        {/* Results */}
        {result && !loading && (
          <>
            {/* Stats Bar */}
            <div className="grid grid-cols-4 gap-3 px-5 py-4 bg-[var(--bg-secondary)] border-b border-[var(--border)]">
              <div className="flex items-center gap-2.5 p-2.5 rounded-lg bg-[var(--bg-primary)] border border-[var(--border)]">
                <Database className="w-5 h-5 text-[var(--accent)]" />
                <div>
                  <p className="text-lg font-bold text-[var(--text-primary)]">{result.db_size_mb.toFixed(1)}</p>
                  <p className="text-[10px] text-[var(--text-muted)] uppercase tracking-wider">Size (MB)</p>
                </div>
              </div>
              <div className="flex items-center gap-2.5 p-2.5 rounded-lg bg-[var(--bg-primary)] border border-[var(--border)]">
                <Table2 className="w-5 h-5 text-blue-400" />
                <div>
                  <p className="text-lg font-bold text-[var(--text-primary)]">{result.total_tables}</p>
                  <p className="text-[10px] text-[var(--text-muted)] uppercase tracking-wider">Tables</p>
                </div>
              </div>
              <div className="flex items-center gap-2.5 p-2.5 rounded-lg bg-[var(--bg-primary)] border border-[var(--border)]">
                <AlertTriangle className="w-5 h-5 text-amber-400" />
                <div>
                  <p className="text-lg font-bold text-[var(--text-primary)]">{result.total_issues}</p>
                  <p className="text-[10px] text-[var(--text-muted)] uppercase tracking-wider">Issues</p>
                </div>
              </div>
              <div className="flex items-center gap-2.5 p-2.5 rounded-lg bg-[var(--bg-primary)] border border-[var(--border)]">
                <HardDrive className="w-5 h-5 text-green-400" />
                <div>
                  <p className="text-lg font-bold text-[var(--text-primary)]">{database}</p>
                  <p className="text-[10px] text-[var(--text-muted)] uppercase tracking-wider">Database</p>
                </div>
              </div>
            </div>

            {/* Summary */}
            <div className="px-5 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]/50">
              <p className="text-sm text-[var(--text-secondary)] italic">
                "{result.summary}"
              </p>
            </div>

            {/* Filters */}
            <div className="flex items-center gap-3 px-5 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]/30">
              <div className="relative flex-1">
                <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-[var(--text-muted)]" />
                <input
                  type="text"
                  placeholder="Search issues..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="w-full pl-8 pr-3 py-1.5 text-sm bg-[var(--bg-primary)] border border-[var(--border)] rounded-md text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)] transition-colors"
                />
              </div>

              {/* Severity filter */}
              <div className="flex items-center gap-1">
                {[
                  { key: "critical", label: "Critical", count: severityCounts.critical, color: "text-red-400", bg: "bg-red-500/10" },
                  { key: "warning", label: "Warning", count: severityCounts.warning, color: "text-amber-400", bg: "bg-amber-500/10" },
                  { key: "info", label: "Info", count: severityCounts.info, color: "text-blue-400", bg: "bg-blue-500/10" },
                ].map(({ key, label, count, color, bg }) => (
                  <button
                    key={key}
                    onClick={() => setFilterSeverity(filterSeverity === key ? null : key)}
                    className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-xs font-medium transition-all ${
                      filterSeverity === key
                        ? `${bg} ${color} ring-1 ring-current`
                        : "text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)]"
                    }`}
                  >
                    <span>{label}</span>
                    <span className={`px-1 rounded text-[10px] ${filterSeverity === key ? "" : "bg-[var(--bg-tertiary)]"}`}>
                      {count}
                    </span>
                  </button>
                ))}
              </div>

              {/* Category filter */}
              {categories.length > 0 && (
                <select
                  value={filterCategory || ""}
                  onChange={(e) => setFilterCategory(e.target.value || null)}
                  className="px-2.5 py-1.5 text-xs bg-[var(--bg-primary)] border border-[var(--border)] rounded-md text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent)] transition-colors"
                >
                  <option value="">All Categories</option>
                  {categories.map((cat) => (
                    <option key={cat} value={cat}>
                      {cat}
                    </option>
                  ))}
                </select>
              )}

              <button
                onClick={runAnalysis}
                disabled={loading}
                className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors"
                title="Re-run analysis"
              >
                <RefreshCw className={`w-4 h-4 ${loading ? "animate-spin" : ""}`} />
              </button>
            </div>

            {/* Issues list */}
            <div className="flex-1 overflow-auto p-5">
              {filteredIssues.length === 0 ? (
                <div className="text-center py-12">
                  <p className="text-sm text-[var(--text-muted)]">No issues match your filters.</p>
                </div>
              ) : (
                <div className="space-y-6">
                  {/* Critical */}
                  {groupedBySeverity.critical.length > 0 && (
                    <div>
                      <h3 className="text-xs font-semibold text-red-400 uppercase tracking-wider mb-3 flex items-center gap-2">
                        <AlertCircle className="w-3.5 h-3.5" />
                        Critical ({groupedBySeverity.critical.length})
                      </h3>
                      <div className="space-y-2">
                        {groupedBySeverity.critical.map((issue, i) => (
                          <IssueCard key={`critical-${i}`} issue={issue} defaultExpanded />
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Warning */}
                  {groupedBySeverity.warning.length > 0 && (
                    <div>
                      <h3 className="text-xs font-semibold text-amber-400 uppercase tracking-wider mb-3 flex items-center gap-2">
                        <AlertTriangle className="w-3.5 h-3.5" />
                        Warnings ({groupedBySeverity.warning.length})
                      </h3>
                      <div className="space-y-2">
                        {groupedBySeverity.warning.map((issue, i) => (
                          <IssueCard key={`warning-${i}`} issue={issue} />
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Info */}
                  {groupedBySeverity.info.length > 0 && (
                    <div>
                      <h3 className="text-xs font-semibold text-blue-400 uppercase tracking-wider mb-3 flex items-center gap-2">
                        <Info className="w-3.5 h-3.5" />
                        Info ({groupedBySeverity.info.length})
                      </h3>
                      <div className="space-y-2">
                        {groupedBySeverity.info.map((issue, i) => (
                          <IssueCard key={`info-${i}`} issue={issue} />
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>

            {/* Footer */}
            <div className="flex items-center justify-between px-5 py-2.5 border-t border-[var(--border)] bg-[var(--bg-secondary)] text-xs text-[var(--text-muted)]">
              <span>Analysis for <span className="font-mono text-[var(--text-secondary)]">{database}</span></span>
              <span className="flex items-center gap-1">
                <Link className="w-3 h-3" />
                Powered by LLM analysis
              </span>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
