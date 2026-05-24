import { useState, useMemo, useEffect, useRef } from "react";
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
  Minimize2,
  X,
  AlertCircle,
  BookOpen,
  Wrench,
  XCircle,
  Zap,
  GitBranch,
  Shield,
  Pencil,
  Square,
  Loader,
} from "lucide-react";
import { nlToSql, executeSql, explainSql, explainSqlNatural, fixSql, optimizeSql, schemaMigration, nlDataEdit, executeSqlStreaming, cancelRunningQuery, setupStreamListeners, cleanupStreamListeners } from "../api";
import { JoinBuilder } from "./JoinBuilder";
import { ToolCallTrace } from "./ToolCallTrace";
import { MigrationPreview } from "./MigrationPreview";
import { DataEditPreview } from "./DataEditPreview";
import type { QueryResult, Schema, ToolCallStep, SchemaMigrationResponse, DataEditResponse } from "../types";

interface QueryEditorProps {
  onResult: (result: QueryResult) => void;
  schema: Schema | null;
  selectedDatabase: string | null;
  tableNames?: string[];
  initialSql?: string;
  initialNaturalLanguage?: string;
  onSqlChange?: (sql: string) => void;
  onNlChange?: (nl: string) => void;
  onToolSteps?: (steps: ToolCallStep[], iterations: number, fallback: boolean) => void;
  onCollapse?: () => void;
}

export function QueryEditor({
  onResult,
  schema,
  selectedDatabase,
  tableNames = [],
  initialSql = "",
  initialNaturalLanguage = "",
  onSqlChange,
  onNlChange,
  onToolSteps,
  onCollapse,
}: QueryEditorProps) {
  const [naturalLanguage, setNaturalLanguage] = useState(initialNaturalLanguage);
  const [sqlText, setSqlText] = useState(initialSql);
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
  const [sqlExplanation, setSqlExplanation] = useState("");
  const [isExplaining, setIsExplaining] = useState(false);
  const [sqlFix, setSqlFix] = useState<{ fixed: string; explanation: string } | null>(null);
  const [isFixing, setIsFixing] = useState(false);
  const [sqlOptimization, setSqlOptimization] = useState<{ suggestions: string; optimized_sql: string | null } | null>(null);
  const [isOptimizing, setIsOptimizing] = useState(false);
  const [showJoinBuilder, setShowJoinBuilder] = useState(false);
  const [toolSteps, setToolSteps] = useState<ToolCallStep[]>([]);
  const [toolIterations, setToolIterations] = useState(0);
  const [toolFallback, setToolFallback] = useState(false);
  const [editorMode, setEditorMode] = useState<"query" | "schema" | "edit">("query");
  const [migrationResult, setMigrationResult] = useState<SchemaMigrationResponse | null>(null);
  const [isMigrationLoading, setIsMigrationLoading] = useState(false);
  const [showMigrationPreview, setShowMigrationPreview] = useState(false);
  const [dataEditResult, setDataEditResult] = useState<DataEditResponse | null>(null);
  const [isDataEditLoading, setIsDataEditLoading] = useState(false);
  const [showDataEditPreview, setShowDataEditPreview] = useState(false);
  const [streaming, setStreaming] = useState(false);
  const [streamingProgress, setStreamingProgress] = useState(0);
  const [streamQueryId, setStreamQueryId] = useState<string | null>(null);
  const streamingRowsRef = useRef<(string | number | boolean | null)[][]>([]);

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

  // Update SQL when initialSql changes (from ResultActions)
  useEffect(() => {
    if (initialSql) {
      setSqlText(initialSql);
    }
  }, [initialSql]);

  // Cleanup stream listeners on unmount
  useEffect(() => {
    return () => {
      cleanupStreamListeners();
    };
  }, []);

  const handleSetSql = (val: string) => {
    setSqlText(val);
    onSqlChange?.(val);
  };

  const handleSetNl = (val: string) => {
    setNaturalLanguage(val);
    onNlChange?.(val);
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

  const handleTranslate = async () => {
    if (!naturalLanguage.trim()) return;

    setIsTranslating(true);
    setError("");

    try {
      const response = await nlToSql({
        natural_language: naturalLanguage,
        database: selectedDatabase || "",
      });
      setSqlText(response.sql);
      setToolSteps(response.tool_calls);
      setToolIterations(response.iterations);
      setToolFallback(response.used_fallback);
      onSqlChange?.(response.sql);
      onToolSteps?.(response.tool_calls, response.iterations, response.used_fallback);
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Translation failed");
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
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Query execution failed");
    } finally {
      setIsExecuting(false);
    }
  };

  const handleExecuteStreaming = async () => {
    if (!sqlText.trim()) return;

    const queryId = crypto.randomUUID();
    setStreamQueryId(queryId);
    setStreaming(true);
    setStreamingProgress(0);
    setError("");
    streamingRowsRef.current = [];

    try {
      await setupStreamListeners(
        queryId,
        (columns, rows, totalSoFar) => {
          streamingRowsRef.current.push(...rows as (string | number | boolean | null)[][]);
          setStreamingProgress(totalSoFar);
          onResult({
            columns,
            rows: [...streamingRowsRef.current],
            row_count: totalSoFar,
            execution_time_ms: 0,
            affected_rows: null,
          });
        },
        (_totalDone) => {
          setStreaming(false);
          setStreamQueryId(null);
          cleanupStreamListeners();
          setIsExecuting(false);
        },
        (error) => {
          setStreaming(false);
          setStreamQueryId(null);
          cleanupStreamListeners();
          setError(error);
          setIsExecuting(false);
        },
      );

      await executeSqlStreaming(sqlText, queryId);
    } catch (err) {
      setStreaming(false);
      setStreamQueryId(null);
      cleanupStreamListeners();
      // Error is already reported through the stream-error event
      if (!streamingRowsRef.current.length) {
        setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Streaming execution failed");
      }
      setIsExecuting(false);
    }
  };

  const handleCancelStreaming = async () => {
    if (streamQueryId) {
      await cancelRunningQuery(streamQueryId);
      setStreaming(false);
      setStreamQueryId(null);
      cleanupStreamListeners();
      setIsExecuting(false);
    }
  };

  const handleExplain = async () => {
    if (!sqlText.trim()) return;

    setIsExecuting(true);
    setError("");

    try {
      const result = await explainSql({ sql: sqlText });
      onResult(result);
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Explain failed");
    } finally {
      setIsExecuting(false);
    }
  };

  const handleExplainNatural = async () => {
    if (!sqlText.trim()) return;

    setIsExplaining(true);
    setSqlExplanation("");
    setError("");

    try {
      const response = await explainSqlNatural({ sql: sqlText });
      setSqlExplanation(response.explanation);
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Failed to explain query");
    } finally {
      setIsExplaining(false);
    }
  };

  const handleFixSql = async () => {
    if (!sqlText.trim() || !error) return;

    setIsFixing(true);
    setSqlFix(null);

    try {
      const response = await fixSql({ sql: sqlText, error });
      setSqlFix({ fixed: response.fixed_sql, explanation: response.explanation });
      handleSetSql(response.fixed_sql);
    } catch (err) {
    } finally {
      setIsFixing(false);
    }
  };

  const handleOptimize = async () => {
    if (!sqlText.trim()) return;

    setIsOptimizing(true);
    setSqlOptimization(null);

    try {
      const response = await optimizeSql({ sql: sqlText });
      setSqlOptimization({ suggestions: response.suggestions, optimized_sql: response.optimized_sql });
      if (response.optimized_sql) {
        handleSetSql(response.optimized_sql);
      }
    } catch (err) {
    } finally {
      setIsOptimizing(false);
    }
  };

  const handleCopy = () => {
    navigator.clipboard.writeText(sqlText);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleClear = () => {
    handleSetSql("");
    handleSetNl("");
    setError("");
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      if (editorMode === "query") {
        handleExecute();
      } else {
        handleSchemaMigration();
      }
    }
  };

  const handleSchemaMigration = async () => {
    if (!naturalLanguage.trim()) return;
    setIsMigrationLoading(true);
    setMigrationResult(null);
    setShowMigrationPreview(true);
    try {
      const response = await schemaMigration(naturalLanguage, selectedDatabase || "");
      setMigrationResult(response);
      handleSetSql(response.sql);
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Migration generation failed");
      setShowMigrationPreview(false);
    } finally {
      setIsMigrationLoading(false);
    }
  };

  const handleDataEdit = async () => {
    if (!naturalLanguage.trim()) return;
    setIsDataEditLoading(true);
    setDataEditResult(null);
    setShowDataEditPreview(true);
    try {
      const response = await nlDataEdit(naturalLanguage, selectedDatabase || "");
      setDataEditResult(response);
      handleSetSql(response.sql);
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Data edit generation failed");
      setShowDataEditPreview(false);
    } finally {
      setIsDataEditLoading(false);
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
            onChange={(val) => handleSetSql(val)}
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
            onClick={handleExecuteStreaming}
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
    <div className="flex flex-col h-full">
      {/* NL Input Bar */}
      <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--border)] bg-[var(--bg-secondary)] shrink-0">
        <div className="flex items-center gap-0.5 border border-[var(--border)] rounded-md p-0.5">
          <button
            onClick={() => setEditorMode("query")}
            className={`p-1 rounded transition-colors ${editorMode === "query" ? "bg-[var(--accent)]/10 text-[var(--accent)]" : "text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)]"}`}
            title="Query Mode"
          >
            <Wand2 className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={() => setEditorMode("schema")}
            className={`p-1 rounded transition-colors ${editorMode === "schema" ? "bg-[var(--accent)]/10 text-[var(--accent)]" : "text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)]"}`}
            title="Schema Mode (DDL)"
          >
            <Shield className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={() => setEditorMode("edit")}
            className={`p-1 rounded transition-colors ${editorMode === "edit" ? "bg-[var(--accent)]/10 text-[var(--accent)]" : "text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)]"}`}
            title="Data Edit Mode (DML)"
          >
            <Pencil className="w-3.5 h-3.5" />
          </button>
        </div>
        <input
          type="text"
          value={naturalLanguage}
          onChange={(e) => handleSetNl(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              if (editorMode === "query") handleTranslate();
              else if (editorMode === "schema") handleSchemaMigration();
              else handleDataEdit();
            }
          }}
          placeholder={
            editorMode === "query" ? "Describe your query in natural language..." :
            editorMode === "schema" ? "Describe schema changes (e.g. 'add email column to users')..." :
            "Describe data changes (e.g. 'set user 5 status to active')..."
          }
          className="flex-1 px-3 py-1.5 rounded-md bg-transparent text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none text-sm"
        />
        <button
          onClick={
            editorMode === "query" ? handleTranslate :
            editorMode === "schema" ? handleSchemaMigration :
            handleDataEdit
          }
          disabled={
            (editorMode === "query" && (isTranslating || !naturalLanguage.trim())) ||
            (editorMode === "schema" && (isMigrationLoading || !naturalLanguage.trim())) ||
            (editorMode === "edit" && (isDataEditLoading || !naturalLanguage.trim()))
          }
          className="px-3 py-1.5 rounded-md bg-[var(--accent)]/10 text-[var(--accent)] text-sm font-medium hover:bg-[var(--accent)]/20 disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
        >
          {(editorMode === "query" && isTranslating) || (editorMode === "schema" && isMigrationLoading) || (editorMode === "edit" && isDataEditLoading) ? (
            <Loader2 className="w-3.5 h-3.5 animate-spin" />
          ) : editorMode === "query" ? (
            <Sparkles className="w-3.5 h-3.5" />
          ) : editorMode === "schema" ? (
            <Shield className="w-3.5 h-3.5" />
          ) : (
            <Pencil className="w-3.5 h-3.5" />
          )}
          {editorMode === "query" ? "Translate" : editorMode === "schema" ? "Generate DDL" : "Generate DML"}
        </button>
      </div>

      {/* SQL Editor */}
      <div className="flex-1 min-h-0 overflow-hidden">
        <CodeMirror
          value={sqlText}
          onChange={(val) => handleSetSql(val)}
          onKeyDown={handleKeyDown}
          extensions={extensions}
          theme={isDark ? vscodeDark : vscodeLight}
          height="100%"
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

      {/* Tool Call Trace */}
      <ToolCallTrace steps={toolSteps} iterations={toolIterations} usedFallback={toolFallback} />

      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-2 border-t border-[var(--border)] bg-[var(--bg-secondary)] shrink-0 gap-2 overflow-x-auto">
        <div className="flex items-center gap-1 shrink-0">
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
          {onCollapse && (
            <button
              onClick={onCollapse}
              className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] transition-colors"
              title="Collapse editor (focus on results)"
            >
              <Minimize2 className="w-4 h-4" />
            </button>
          )}
        </div>
        <div className="flex items-center gap-1.5 flex-wrap justify-end">
          <button
            onClick={handleExplain}
            disabled={isExecuting || !sqlText.trim()}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            title="EXPLAIN"
          >
            {isExecuting ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <AlertCircle className="w-4 h-4" />
            )}
          </button>
          <button
            onClick={handleExplainNatural}
            disabled={isExplaining || !sqlText.trim()}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            title="AI Explain"
          >
            {isExplaining ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <BookOpen className="w-4 h-4" />
            )}
          </button>
          <button
            onClick={handleOptimize}
            disabled={isOptimizing || !sqlText.trim()}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            title="Optimize"
          >
            {isOptimizing ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Zap className="w-4 h-4" />
            )}
          </button>
          <button
            onClick={() => setShowJoinBuilder(true)}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] transition-colors"
            title="Join Builder"
          >
            <GitBranch className="w-4 h-4" />
          </button>
          <div className="w-px h-5 bg-[var(--border)] mx-0.5" />
          {streaming ? (
            <div className="flex items-center gap-2">
              <div className="flex items-center gap-1 text-xs text-[var(--accent)]">
                <Loader className="w-3 h-3 animate-spin" />
                <span>{streamingProgress.toLocaleString()} rows loaded</span>
              </div>
              <button
                onClick={handleCancelStreaming}
                className="px-2 py-1.5 rounded-md bg-red-500/10 text-red-400 text-sm hover:bg-red-500/20 transition-colors flex items-center gap-1"
                title="Cancel query"
              >
                <Square className="w-3.5 h-3.5" />
                Cancel
              </button>
            </div>
          ) : (
            <button
              onClick={handleExecuteStreaming}
              disabled={isExecuting || !sqlText.trim()}
              className="px-3 py-1.5 rounded-md bg-[var(--success)] text-white text-sm font-medium hover:bg-green-600 disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5 shrink-0"
              title="Run streaming (Ctrl+Shift+Enter)"
            >
              {isExecuting ? (
                <Loader2 className="w-4 h-4 animate-spin" />
              ) : (
                <Send className="w-4 h-4" />
              )}
              Run
            </button>
          )}
        </div>
      </div>

      {/* Error Bar */}
      {error && (
        <div className="px-4 py-2 border-t border-red-500/20 bg-red-500/5 shrink-0">
          <div className="flex items-start justify-between gap-2">
            <div className="flex-1">
              <div className="flex items-center gap-2 mb-1">
                <XCircle className="w-4 h-4 text-red-500 flex-shrink-0" />
                <span className="text-sm font-medium text-red-500">Query Error</span>
              </div>
              <p className="text-xs text-red-400/80 whitespace-pre-wrap">{error}</p>
            </div>
            <button
              onClick={handleFixSql}
              disabled={isFixing}
              className="px-3 py-1.5 rounded-md bg-red-500/20 text-red-400 text-xs font-medium hover:bg-red-500/30 disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5 shrink-0"
            >
              {isFixing ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Wrench className="w-3.5 h-3.5" />
              )}
              Fix with AI
            </button>
          </div>
        </div>
      )}

      {/* AI Fix Result */}
      {sqlFix && (
        <div className="px-4 py-3 border-t border-[var(--border)] bg-green-500/5 shrink-0">
          <div className="flex items-start justify-between gap-2 mb-1">
            <div className="flex items-center gap-2">
              <Wrench className="w-4 h-4 text-green-500" />
              <span className="text-sm font-medium text-green-500">Fixed by AI</span>
            </div>
            <button
              onClick={() => setSqlFix(null)}
              className="p-1 rounded hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] transition-colors"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
          {sqlFix.explanation && (
            <p className="text-xs text-[var(--text-secondary)] mb-2">{sqlFix.explanation}</p>
          )}
        </div>
      )}

      {/* AI Explanation */}
      {sqlExplanation && (
        <div className="px-4 py-3 border-t border-[var(--border)] bg-[var(--bg-secondary)] shrink-0">
          <div className="flex items-start justify-between gap-2">
            <div className="flex items-center gap-2 mb-1">
              <BookOpen className="w-4 h-4 text-[var(--accent)]" />
              <span className="text-sm font-medium text-[var(--text-primary)]">AI Explanation</span>
            </div>
            <button
              onClick={() => setSqlExplanation("")}
              className="p-1 rounded hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] transition-colors"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
          <p className="text-sm text-[var(--text-secondary)] leading-relaxed whitespace-pre-wrap">
            {sqlExplanation}
          </p>
        </div>
      )}

      {/* AI Optimization */}
      {sqlOptimization && (
        <div className="px-4 py-3 border-t border-[var(--border)] bg-[var(--bg-secondary)] shrink-0">
          <div className="flex items-start justify-between gap-2 mb-2">
            <div className="flex items-center gap-2">
              <Zap className="w-4 h-4 text-yellow-500" />
              <span className="text-sm font-medium text-yellow-500">Optimization Analysis</span>
            </div>
            <button
              onClick={() => setSqlOptimization(null)}
              className="p-1 rounded hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] transition-colors"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
          {sqlOptimization.optimized_sql && (
            <div className="mb-2 px-3 py-2 rounded-md bg-[var(--bg-primary)] border border-[var(--border)] font-mono text-xs text-[var(--text-primary)] whitespace-pre-wrap">
              {sqlOptimization.optimized_sql}
            </div>
          )}
          <p className="text-sm text-[var(--text-secondary)] leading-relaxed whitespace-pre-wrap">
            {sqlOptimization.suggestions}
          </p>
        </div>
      )}

      {/* Join Builder Modal */}
      <JoinBuilder
        isOpen={showJoinBuilder}
        onClose={() => setShowJoinBuilder(false)}
        onApply={(sql) => {
          handleSetSql(sql);
          setShowJoinBuilder(false);
        }}
        tableNames={tableNames}
      />

      {/* Migration Preview Modal */}
      <MigrationPreview
        isOpen={showMigrationPreview}
        onClose={() => setShowMigrationPreview(false)}
        migration={migrationResult}
        loading={isMigrationLoading}
        onApply={() => setShowMigrationPreview(false)}
      />

      {/* Data Edit Preview Modal */}
      <DataEditPreview
        isOpen={showDataEditPreview}
        onClose={() => setShowDataEditPreview(false)}
        edit={dataEditResult}
        loading={isDataEditLoading}
        onApply={() => setShowDataEditPreview(false)}
      />
    </div>
  );
}
