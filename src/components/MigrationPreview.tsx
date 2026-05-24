import { useState } from "react";
import { X, AlertTriangle, CheckCircle, Loader2, Shield, Play } from "lucide-react";
import { executeSql } from "../api";
import type { SchemaMigrationResponse } from "../types";

interface MigrationPreviewProps {
  isOpen: boolean;
  onClose: () => void;
  migration: SchemaMigrationResponse | null;
  loading: boolean;
  onApply: () => void;
}

export function MigrationPreview({ isOpen, onClose, migration, loading, onApply }: MigrationPreviewProps) {
  const [executing, setExecuting] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState("");

  if (!isOpen) return null;

  const handleExecute = async () => {
    if (!migration) return;
    setExecuting(true);
    setError("");
    setResult(null);
    try {
      const res = await executeSql({ sql: migration.sql });
      setResult(`Success. ${res.affected_rows !== null ? res.affected_rows + " rows affected" : res.row_count + " rows returned"}`);
      onApply();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setExecuting(false);
    }
  };

  const riskConfig = {
    high: { color: "text-red-400", bg: "bg-red-500/10 border-red-500/30", label: "High Risk" },
    medium: { color: "text-orange-400", bg: "bg-orange-500/10 border-orange-500/30", label: "Medium Risk" },
    low: { color: "text-green-400", bg: "bg-green-500/10 border-green-500/30", label: "Low Risk" },
  };
  const risk = riskConfig[migration?.risk_level as keyof typeof riskConfig] || riskConfig.low;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="w-[600px] max-h-[80vh] bg-[var(--bg-primary)] border border-[var(--border)] rounded-xl shadow-2xl flex flex-col overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
          <div className="flex items-center gap-2">
            <Shield className="w-5 h-5 text-[var(--accent)]" />
            <h2 className="text-sm font-semibold text-[var(--text-primary)]">Schema Migration Preview</h2>
          </div>
          <button onClick={onClose} className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)]">
            <X className="w-4 h-4 text-[var(--text-muted)]" />
          </button>
        </div>

        {/* Loading */}
        {loading && (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="w-5 h-5 animate-spin text-[var(--accent)]" />
            <span className="ml-2 text-sm text-[var(--text-muted)]">Generating migration SQL...</span>
          </div>
        )}

        {/* Migration Content */}
        {migration && !loading && (
          <div className="flex-1 overflow-auto p-5 space-y-4">
            {/* Risk Level */}
            <div className={`flex items-center gap-2 px-3 py-2 rounded-lg border ${risk.bg}`}>
              {migration.risk_level === "high" ? (
                <AlertTriangle className={`w-4 h-4 ${risk.color}`} />
              ) : (
                <CheckCircle className={`w-4 h-4 ${risk.color}`} />
              )}
              <span className={`text-sm font-medium ${risk.color}`}>{risk.label}</span>
              {migration.risk_level === "high" && (
                <span className="text-xs text-[var(--text-muted)] ml-2">This operation may cause data loss</span>
              )}
            </div>

            {/* SQL */}
            <div>
              <label className="text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider mb-1.5 block">Generated SQL</label>
              <div className="p-3 rounded-md bg-[var(--bg-tertiary)] border border-[var(--border)] font-mono text-sm text-[var(--text-primary)] whitespace-pre-wrap max-h-48 overflow-auto">
                {migration.sql}
              </div>
            </div>

            {/* Explanation */}
            {migration.explanation && (
              <div>
                <label className="text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider mb-1.5 block">Explanation</label>
                <p className="text-sm text-[var(--text-secondary)] leading-relaxed">{migration.explanation}</p>
              </div>
            )}

            {/* Result */}
            {result && (
              <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-green-500/10 border border-green-500/30">
                <CheckCircle className="w-4 h-4 text-green-400" />
                <span className="text-sm text-green-400">{result}</span>
              </div>
            )}
            {error && (
              <div className="px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/30">
                <p className="text-sm text-red-400">{error}</p>
              </div>
            )}
          </div>
        )}

        {/* Footer */}
        {migration && !loading && !result && (
          <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-[var(--border)] bg-[var(--bg-secondary)]">
            <button
              onClick={onClose}
              className="px-4 py-1.5 rounded-md text-sm text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)] transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleExecute}
              disabled={executing}
              className="px-4 py-1.5 rounded-md bg-[var(--accent)] text-white text-sm font-medium hover:opacity-90 disabled:opacity-40 transition-colors flex items-center gap-1.5"
            >
              {executing ? (
                <Loader2 className="w-4 h-4 animate-spin" />
              ) : (
                <Play className="w-4 h-4" />
              )}
              Execute Migration
            </button>
          </div>
        )}

        {result && (
          <div className="flex items-center justify-end px-5 py-3 border-t border-[var(--border)] bg-[var(--bg-secondary)]">
            <button onClick={onClose} className="px-4 py-1.5 rounded-md text-sm text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)]">
              Close
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
