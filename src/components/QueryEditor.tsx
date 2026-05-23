import { useState } from "react";
import { Send, Sparkles, Loader2, ArrowRight, Copy, Check } from "lucide-react";
import { nlToSql, executeSql } from "../api";
import type { QueryResult } from "../types";

interface QueryEditorProps {
  onResult: (result: QueryResult) => void;
}

export function QueryEditor({ onResult }: QueryEditorProps) {
  const [naturalLanguage, setNaturalLanguage] = useState("");
  const [sql, setSql] = useState("");
  const [isTranslating, setIsTranslating] = useState(false);
  const [isExecuting, setIsExecuting] = useState(false);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);

  const handleTranslate = async () => {
    if (!naturalLanguage.trim()) return;

    setIsTranslating(true);
    setError("");

    try {
      const response = await nlToSql({ natural_language: naturalLanguage });
      setSql(response.sql);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Translation failed");
    } finally {
      setIsTranslating(false);
    }
  };

  const handleExecute = async (query?: string) => {
    const queryToRun = query || sql;
    if (!queryToRun.trim()) return;

    setIsExecuting(true);
    setError("");

    try {
      const result = await executeSql({ sql: queryToRun });
      onResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Query execution failed");
    } finally {
      setIsExecuting(false);
    }
  };

  const handleCopy = () => {
    navigator.clipboard.writeText(sql);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      handleExecute();
    }
  };

  return (
    <div className="space-y-3">
      {/* Natural Language Input */}
      <div className="space-y-2">
        <label className="text-sm font-medium text-[var(--text-secondary)] flex items-center gap-2">
          <Sparkles className="w-4 h-4 text-[var(--accent)]" />
          Natural Language Query
        </label>
        <div className="flex gap-2">
          <input
            type="text"
            value={naturalLanguage}
            onChange={(e) => setNaturalLanguage(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleTranslate()}
            placeholder="e.g., show all users who registered last month"
            className="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
          />
          <button
            onClick={handleTranslate}
            disabled={isTranslating || !naturalLanguage.trim()}
            className="px-4 py-2 rounded-lg bg-[var(--accent)] text-white font-medium hover:bg-[var(--accent-hover)] disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center gap-2"
          >
            {isTranslating ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Sparkles className="w-4 h-4" />
            )}
            Translate
          </button>
        </div>
      </div>

      {/* Arrow connector */}
      <div className="flex items-center justify-center">
        <ArrowRight className="w-4 h-4 text-[var(--text-muted)]" />
      </div>

      {/* SQL Preview */}
      <div className="space-y-2">
        <label className="text-sm font-medium text-[var(--text-secondary)] flex items-center gap-2">
          <span className="font-mono text-[var(--accent)]">SQL</span>
          Preview
        </label>
        <div className="relative">
          <textarea
            value={sql}
            onChange={(e) => setSql(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="SELECT * FROM ..."
            rows={4}
            className="w-full px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] font-mono text-sm resize-none pr-20"
          />
          <div className="absolute top-2 right-2 flex gap-1">
            <button
              onClick={handleCopy}
              disabled={!sql}
              className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] disabled:opacity-30 transition-colors"
              title="Copy SQL"
            >
              {copied ? (
                <Check className="w-4 h-4 text-[var(--success)]" />
              ) : (
                <Copy className="w-4 h-4" />
              )}
            </button>
          </div>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-xs text-[var(--text-muted)]">
            Ctrl+Enter to execute
          </span>
          <button
            onClick={() => handleExecute()}
            disabled={isExecuting || !sql.trim()}
            className="px-4 py-2 rounded-lg bg-[var(--success)] text-white font-medium hover:bg-green-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center gap-2"
          >
            {isExecuting ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Send className="w-4 h-4" />
            )}
            Execute
          </button>
        </div>
      </div>

      {/* Error Display */}
      {error && (
        <div className="px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20 text-red-500 text-sm">
          {error}
        </div>
      )}
    </div>
  );
}
