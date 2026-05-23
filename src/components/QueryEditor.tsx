import { useState, useMemo, useEffect, useCallback, useRef } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { sql, MySQL } from "@codemirror/lang-sql";
import { vscodeDark, vscodeLight } from "@uiw/codemirror-theme-vscode";
import {
  Sparkles,
  Loader2,
  Send,
  Copy,
  Check,
  Eraser,
  Play,
  Wand2,
  Maximize2,
  X,
} from "lucide-react";
import { nlToSql, executeSql } from "../api";
import type { QueryResult, Schema } from "../types";

interface QueryEditorProps {
  onResult: (result: QueryResult) => void;
  schema: Schema | null;
  selectedDatabase: string | null;
}

export function QueryEditor({
  onResult,
  schema,
  selectedDatabase,
}: QueryEditorProps) {
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
  const [expanded, setExpanded] = useState(false);
  const [editorHeight, setEditorHeight] = useState(240);
  const [isDragging, setIsDragging] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Track theme changes
  useEffect(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains("dark"));
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });
    return () => observer.disconnect();
  }, []);

  // Resize drag handler
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsDragging(true);
  }, []);

  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      if (!containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      const newHeight = e.clientY - rect.top;
      setEditorHeight(Math.max(120, Math.min(600, newHeight)));
    };

    const handleMouseUp = () => setIsDragging(false);

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    document.body.style.userSelect = "none";
    document.body.style.cursor = "row-resize";

    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
    };
  }, [isDragging]);

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

  const handleClear = () => {
    setSqlText("");
    setNaturalLanguage("");
    setError("");
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      handleExecute();
    }
  };

  // Expanded (fullscreen) editor
  if (expanded) {
    return (
      <div className="fixed inset-0 z-50 bg-[var(--bg-primary)] flex flex-col">
        {/* Expanded header */}
        <div className="flex items-center justify-between px-4 py-2 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
          <div className="flex items-center gap-2">
            <Wand2 className="w-4 h-4 text-[var(--accent)]" />
            <span className="text-sm font-medium text-[var(--text-secondary)]">
              SQL Editor
            </span>
          </div>
          <button
            onClick={() => setExpanded(false)}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] transition-colors"
          >
            <X className="w-4 h-4 text-[var(--text-muted)]" />
          </button>
        </div>

        {/* Full-screen editor */}
        <div className="flex-1">
          <CodeMirror
            value={sqlText}
            onChange={(val) => setSqlText(val)}
            onKeyDown={handleKeyDown}
            extensions={extensions}
            theme={isDark ? vscodeDark : vscodeLight}
            height="100%"
            className="text-sm [&_.cm-editor]:!h-full [&_.cm-scroller]:!overflow-auto [&_.cm-content]:!text-[15px]"
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
        </div>

        {/* Expanded toolbar */}
        <div className="flex items-center justify-between px-4 py-2 border-t border-[var(--border)] bg-[var(--bg-secondary)]">
          <div className="flex items-center gap-1">
            <button
              onClick={handleCopy}
              disabled={!sqlText}
              className="p-2 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] disabled:opacity-30 transition-colors"
            >
              {copied ? (
                <Check className="w-4 h-4 text-[var(--success)]" />
              ) : (
                <Copy className="w-4 h-4" />
              )}
            </button>
            <button
              onClick={handleClear}
              className="p-2 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] transition-colors"
            >
              <Eraser className="w-4 h-4" />
            </button>
          </div>
          <button
            onClick={handleExecute}
            disabled={isExecuting || !sqlText.trim()}
            className="px-6 py-2 rounded-md bg-[var(--success)] text-white font-medium hover:bg-green-600 disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-2"
          >
            {isExecuting ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Play className="w-4 h-4" />
            )}
            Run Query
          </button>
        </div>
      </div>
    );
  }

  return (
    <div ref={containerRef} className="flex flex-col">
      {/* NL Input Bar */}
      <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
        <Wand2 className="w-4 h-4 text-[var(--accent)] flex-shrink-0" />
        <input
          type="text"
          value={naturalLanguage}
          onChange={(e) => setNaturalLanguage(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleTranslate()}
          placeholder="Describe your query in natural language..."
          className="flex-1 px-3 py-1.5 rounded-md bg-transparent text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none text-sm"
        />
        <button
          onClick={handleTranslate}
          disabled={isTranslating || !naturalLanguage.trim() || !selectedDatabase}
          className="px-3 py-1.5 rounded-md bg-[var(--accent)]/10 text-[var(--accent)] text-sm font-medium hover:bg-[var(--accent)]/20 disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
          title={!selectedDatabase ? "Select a database first" : ""}
        >
          {isTranslating ? (
            <Loader2 className="w-3.5 h-3.5 animate-spin" />
          ) : (
            <Sparkles className="w-3.5 h-3.5" />
          )}
          Translate
        </button>
      </div>

      {/* SQL Editor */}
      <div style={{ height: editorHeight }}>
        <CodeMirror
          value={sqlText}
          onChange={(val) => setSqlText(val)}
          onKeyDown={handleKeyDown}
          extensions={extensions}
          theme={isDark ? vscodeDark : vscodeLight}
          height={`${editorHeight}px`}
          className="text-sm [&_.cm-editor]:!h-full [&_.cm-scroller]:!overflow-auto"
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
      </div>

      {/* Resize Handle */}
      <div
        className="h-1.5 cursor-row-resize group flex items-center justify-center hover:bg-[var(--accent)]/20 transition-colors"
        onMouseDown={handleMouseDown}
      >
        <div className="w-12 h-1 rounded-full bg-[var(--border)] group-hover:bg-[var(--accent)] transition-colors" />
      </div>

      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-2 border-t border-[var(--border)] bg-[var(--bg-secondary)]">
        <div className="flex items-center gap-1">
          <button
            onClick={handleCopy}
            disabled={!sqlText}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] disabled:opacity-30 transition-colors"
            title="Copy SQL"
          >
            {copied ? (
              <Check className="w-4 h-4 text-[var(--success)]" />
            ) : (
              <Copy className="w-4 h-4" />
            )}
          </button>
          <button
            onClick={handleClear}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] transition-colors"
            title="Clear editor"
          >
            <Eraser className="w-4 h-4" />
          </button>
          <button
            onClick={() => setExpanded(true)}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] transition-colors"
            title="Expand editor"
          >
            <Maximize2 className="w-4 h-4" />
          </button>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs text-[var(--text-muted)] flex items-center gap-1">
            <kbd className="px-1.5 py-0.5 rounded bg-[var(--bg-tertiary)] border border-[var(--border)] text-[var(--text-secondary)] font-mono text-[10px]">
              Ctrl
            </kbd>
            +
            <kbd className="px-1.5 py-0.5 rounded bg-[var(--bg-tertiary)] border border-[var(--border)] text-[var(--text-secondary)] font-mono text-[10px]">
              Enter
            </kbd>
            to run
          </span>
          <button
            onClick={handleExecute}
            disabled={isExecuting || !sqlText.trim()}
            className="px-4 py-1.5 rounded-md bg-[var(--success)] text-white text-sm font-medium hover:bg-green-600 disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
          >
            {isExecuting ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Send className="w-4 h-4" />
            )}
            Run
          </button>
        </div>
      </div>

      {/* Error Bar */}
      {error && (
        <div className="px-4 py-2 border-t border-red-500/20 bg-red-500/5 text-red-500 text-sm">
          {error}
        </div>
      )}
    </div>
  );
}
