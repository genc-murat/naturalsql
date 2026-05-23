import { useState, useMemo } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { sql, MySQL } from "@codemirror/lang-sql";
import { vscodeDark, vscodeLight } from "@uiw/codemirror-theme-vscode";
import { Send, Sparkles, Loader2, ArrowRight, Copy, Check } from "lucide-react";
import { nlToSql, executeSql } from "../api";
import type { QueryResult, Schema } from "../types";

interface QueryEditorProps {
  onResult: (result: QueryResult) => void;
  schema: Schema | null;
  selectedDatabase: string | null;
}

export function QueryEditor({ onResult, schema, selectedDatabase }: QueryEditorProps) {
  const [naturalLanguage, setNaturalLanguage] = useState("");
  const [sqlText, setSqlText] = useState("");
  const [isTranslating, setIsTranslating] = useState(false);
  const [isExecuting, setIsExecuting] = useState(false);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);
  const [isDark, setIsDark] = useState(() => {
    const saved = localStorage.getItem("naturalsql-theme");
    if (saved) return saved === "dark";
    return window.matchMedia("(prefers-color-scheme: dark)").matches;
  });

  // Update isDark when theme changes
  useState(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains("dark"));
    });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  });

  const handleTranslate = async () => {
    if (!naturalLanguage.trim()) return;
    if (!selectedDatabase) {
      setError("Please select a database first");
      return;
    }

    setIsTranslating(true);
    setError("");

    try {
      const response = await nlToSql({
        natural_language: naturalLanguage,
        database: selectedDatabase,
      });
      setSqlText(response.sql);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Translation failed");
    } finally {
      setIsTranslating(false);
    }
  };

  const handleExecute = async () => {
    if (!sqlText.trim()) return;

    setIsExecuting(true);
    setError("");

    try {
      const result = await executeSql({ sql: sqlText });
      onResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Query execution failed");
    } finally {
      setIsExecuting(false);
    }
  };

  const handleCopy = () => {
    navigator.clipboard.writeText(sqlText);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const extensions = useMemo(
    () => [
      sql({
        dialect: MySQL,
        upperCaseKeywords: true,
        schema: {
          tables: schema
            ? schema.tables.map((t) => ({
                label: t.name,
                displayLabel: t.name,
                columns: t.columns.map((c) => ({
                  label: c.name,
                  displayLabel: c.name,
                  type: c.column_type,
                })),
              }))
            : [],
        },
      }),
    ],
    [schema]
  );

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
            disabled={isTranslating || !naturalLanguage.trim() || !selectedDatabase}
            className="px-4 py-2 rounded-lg bg-[var(--accent)] text-white font-medium hover:bg-[var(--accent-hover)] disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center gap-2"
            title={!selectedDatabase ? "Select a database first" : ""}
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

      {/* SQL Editor */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-[var(--text-secondary)] flex items-center gap-2">
            <span className="font-mono text-[var(--accent)]">SQL</span>
            Editor
          </label>
          <span className="text-xs text-[var(--text-muted)]">
            Ctrl+Enter to execute
          </span>
        </div>
        <div className="relative rounded-lg overflow-hidden border border-[var(--border)]">
          <CodeMirror
            value={sqlText}
            onChange={(val) => setSqlText(val)}
            extensions={extensions}
            theme={isDark ? vscodeDark : vscodeLight}
            minHeight="120px"
            className="text-sm"
            basicSetup={{
              lineNumbers: true,
              highlightActiveLineGutter: true,
              highlightSpecialChars: true,
              foldGutter: true,
              drawSelection: true,
              dropCursor: true,
              allowMultipleSelections: true,
              indentOnInput: true,
              bracketMatching: true,
              closeBrackets: true,
              autocompletion: true,
              rectangularSelection: true,
              crosshairCursor: true,
              highlightActiveLine: true,
              highlightSelectionMatches: true,
              syntaxHighlighting: true,
              tabSize: 2,
            }}
          />
          <button
            onClick={handleCopy}
            disabled={!sqlText}
            className="absolute top-2 right-2 z-10 p-1.5 rounded-md bg-[var(--bg-secondary)]/80 hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] disabled:opacity-30 transition-colors"
            title="Copy SQL"
          >
            {copied ? (
              <Check className="w-4 h-4 text-[var(--success)]" />
            ) : (
              <Copy className="w-4 h-4" />
            )}
          </button>
        </div>
        <div className="flex justify-end">
          <button
            onClick={handleExecute}
            disabled={isExecuting || !sqlText.trim()}
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
