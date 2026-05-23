import { useState, useEffect, useCallback, useRef } from "react";
import { Database, ExternalLink, Settings, CheckCircle } from "lucide-react";
import { ThemeToggle } from "./components/ThemeToggle";
import { ConnectionModal } from "./components/ConnectionModal";
import { SchemaBrowser } from "./components/SchemaBrowser";
import { QueryEditor } from "./components/QueryEditor";
import { ResultsTable } from "./components/ResultsTable";
import { LlmConfigPanel } from "./components/LlmConfigPanel";
import {
  listDatabases,
  cacheSchema,
  getCachedSchema,
  listCachedDatabases,
  removeCachedSchema,
  getLlmConfig,
} from "./api";
import type { Schema, QueryResult, LlmConfigResponse } from "./types";
import "./App.css";

function App() {
  const [databases, setDatabases] = useState<string[]>([]);
  const [cachedDatabases, setCachedDatabases] = useState<string[]>([]);
  const [selectedDatabase, setSelectedDatabase] = useState<string | null>(null);
  const [schema, setSchema] = useState<Schema | null>(null);
  const [isCaching, setIsCaching] = useState(false);
  const [cachingDatabase, setCachingDatabase] = useState<string | null>(null);
  const [cacheError, setCacheError] = useState("");
  const [queryResult, setQueryResult] = useState<QueryResult | null>(null);
  const [connectionString, setConnectionString] = useState("");
  const [showLlmConfig, setShowLlmConfig] = useState(false);
  const [showConnectionModal, setShowConnectionModal] = useState(false);
  const [llmConfig, setLlmConfig] = useState<LlmConfigResponse | null>(null);
  const [isConnected, setIsConnected] = useState(false);
  const [editorHeight, setEditorHeight] = useState(280);
  const [sidebarWidth, setSidebarWidth] = useState(288); // w-72 = 288px
  const [isDragging, setIsDragging] = useState(false);
  const [isSidebarDragging, setIsSidebarDragging] = useState(false);
  const mainAreaRef = useRef<HTMLDivElement>(null);

  // Load LLM config and cached databases on mount
  useEffect(() => {
    getLlmConfig().then((cfg) => setLlmConfig(cfg)).catch(() => {});
    listCachedDatabases().then((dbs) => setCachedDatabases(dbs)).catch(() => {});
  }, []);

  // Resize drag handler for editor/results divider
  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      if (!mainAreaRef.current) return;
      const rect = mainAreaRef.current.getBoundingClientRect();
      const newHeight = e.clientY - rect.top;
      setEditorHeight(Math.max(150, Math.min(600, newHeight)));
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

  // Sidebar resize drag handler
  useEffect(() => {
    if (!isSidebarDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      setSidebarWidth(Math.max(200, Math.min(500, e.clientX)));
    };

    const handleMouseUp = () => setIsSidebarDragging(false);

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    document.body.style.userSelect = "none";
    document.body.style.cursor = "col-resize";

    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
    };
  }, [isSidebarDragging]);

  // Parse database name from connection string
  const parseDatabaseFromUrl = (url: string): string | null => {
    try {
      const afterAt = url.split('@')[1];
      if (!afterAt) return null;
      const dbPart = afterAt.split('/')[1];
      if (!dbPart) return null;
      return dbPart.split('?')[0] || null;
    } catch {
      return null;
    }
  };

  const handleConnected = useCallback(async () => {
    setIsConnected(true);
    setQueryResult(null);
    setSchema(null);
    setSelectedDatabase(null);
    setCacheError("");

    // Fetch available databases
    try {
      const dbs = await listDatabases();
      setDatabases(dbs);

      // Auto-select database from connection string if specified
      const parsed = parseDatabaseFromUrl(connectionString);
      if (parsed && dbs.includes(parsed)) {
        setSelectedDatabase(parsed);
        // Try to load cached schema
        try {
          const res = await getCachedSchema(parsed);
          if (res.schema) {
            setSchema(res.schema);
          }
        } catch {
          // No cached schema yet
        }
      }
    } catch {
      setDatabases([]);
    }
  }, [connectionString]);

  const handleDisconnected = useCallback(() => {
    setIsConnected(false);
    setDatabases([]);
    setSchema(null);
    setSelectedDatabase(null);
    setQueryResult(null);
    setCacheError("");
  }, []);

  const handleSelectDatabase = useCallback(async (db: string) => {
    setSelectedDatabase(db);

    // Try to load cached schema
    try {
      const res = await getCachedSchema(db);
      if (res.schema) {
        setSchema(res.schema);
      } else {
        setSchema(null);
      }
    } catch {
      setSchema(null);
    }
  }, []);

  const handleCacheDatabase = useCallback(async (db: string) => {
    setIsCaching(true);
    setCachingDatabase(db);
    setCacheError("");

    try {
      const res = await cacheSchema(db);
      if (res.schema) {
        setSchema(res.schema);
        setSelectedDatabase(db);
        // Update cached databases list
        const cached = await listCachedDatabases();
        setCachedDatabases(cached);
      }
    } catch (err) {
      setCacheError(err instanceof Error ? err.message : "Failed to cache schema");
    } finally {
      setIsCaching(false);
      setCachingDatabase(null);
    }
  }, []);

  const handleClearCache = useCallback(async (db: string) => {
    try {
      await removeCachedSchema(db);
      if (selectedDatabase === db) {
        setSchema(null);
        setSelectedDatabase(null);
      }
      const cached = await listCachedDatabases();
      setCachedDatabases(cached);
    } catch (err) {
      setCacheError(err instanceof Error ? err.message : "Failed to clear cache");
    }
  }, [selectedDatabase]);

  const handleResult = useCallback((result: QueryResult) => {
    setQueryResult(result);
  }, []);

  return (
    <div className="h-screen flex flex-col bg-[var(--bg-primary)]">
      {/* Header */}
      <header className="flex items-center gap-4 px-4 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
        <div className="flex items-center gap-2">
          <Database className="w-6 h-6 text-[var(--accent)]" />
          <h1 className="text-lg font-bold text-[var(--text-primary)]">
            Natural<span className="text-[var(--accent)]">SQL</span>
          </h1>
        </div>
        <div className="flex-1 flex justify-center">
          <button
            onClick={() => setShowConnectionModal(true)}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg border transition-colors ${
              isConnected
                ? "border-[var(--success)]/30 bg-[var(--success)]/5 hover:bg-[var(--success)]/10"
                : "border-[var(--border)] bg-[var(--bg-secondary)] hover:bg-[var(--bg-tertiary)]"
            }`}
          >
            <Database className={`w-4 h-4 ${isConnected ? "text-[var(--success)]" : "text-[var(--text-muted)]"}`} />
            <span className={`text-sm ${isConnected ? "text-[var(--success)]" : "text-[var(--text-secondary)]"}`}>
              {isConnected
                ? parseDatabaseFromUrl(connectionString) || parseDatabaseFromUrl(connectionString)?.split(":")[0] || "Connected"
                : "Connect to database"}
            </span>
            {isConnected && <CheckCircle className="w-3.5 h-3.5 text-[var(--success)]" />}
          </button>
        </div>
        <button
          onClick={() => setShowLlmConfig(true)}
          className="p-2 rounded-lg transition-colors hover:bg-[var(--bg-tertiary)]"
          title="LLM Settings"
        >
          <Settings className="w-5 h-5 text-[var(--text-secondary)]" />
        </button>
        <ThemeToggle />
      </header>

      {/* Main Content */}
      <div ref={mainAreaRef} className="flex flex-1 overflow-hidden">
        {/* Sidebar - Schema Browser */}
        <aside
          className="border-r border-[var(--border)] bg-[var(--bg-secondary)] flex flex-col shrink-0"
          style={{ width: sidebarWidth }}
        >
          <div className="px-3 py-2 border-b border-[var(--border)]">
            <h2 className="text-sm font-semibold text-[var(--text-secondary)]">
              Databases
            </h2>
          </div>
          <div className="flex-1 overflow-auto p-2">
            <SchemaBrowser
              schema={schema}
              databases={databases}
              cachedDatabases={cachedDatabases}
              selectedDatabase={selectedDatabase}
              onSelectDatabase={handleSelectDatabase}
              onCacheDatabase={handleCacheDatabase}
              onClearCache={handleClearCache}
              isCaching={isCaching}
              cachingDatabase={cachingDatabase}
            />
          </div>
          {cacheError && (
            <div className="p-3 border-t border-[var(--border)]">
              <p className="text-xs text-[var(--error)] text-center">{cacheError}</p>
            </div>
          )}
        </aside>

        {/* Vertical Resize Divider */}
        <div
          className="w-1.5 cursor-col-resize group flex flex-col items-center justify-center shrink-0 bg-[var(--border)] hover:bg-[var(--accent)]/30 transition-colors"
          onMouseDown={() => setIsSidebarDragging(true)}
        >
          <div className="h-16 w-1 rounded-full bg-[var(--text-muted)]/40 group-hover:bg-[var(--accent)] transition-colors" />
        </div>

        {/* Main Area */}
        <main className="flex-1 flex flex-col overflow-hidden">
          {/* Query Editor Panel */}
          <div className="flex flex-col bg-[var(--bg-primary)]" style={{ height: editorHeight }}>
            <QueryEditor
              onResult={handleResult}
              schema={schema}
              selectedDatabase={selectedDatabase}
            />
          </div>

          {/* Draggable Divider */}
          <div
            className="h-1.5 cursor-row-resize group flex items-center justify-center shrink-0 bg-[var(--border)] hover:bg-[var(--accent)]/30 transition-colors"
            onMouseDown={() => setIsDragging(true)}
          >
            <div className="w-16 h-1 rounded-full bg-[var(--text-muted)]/40 group-hover:bg-[var(--accent)] transition-colors" />
          </div>

          {/* Results Panel */}
          <div className="flex-1 min-h-0 overflow-auto p-3 bg-[var(--bg-primary)]">
            <ResultsTable result={queryResult} />
          </div>
        </main>
      </div>

      {/* Footer */}
      <footer className="px-4 py-2 border-t border-[var(--border)] bg-[var(--bg-secondary)] text-xs text-[var(--text-muted)] flex items-center justify-between">
        <span>
          MySQL 5.6+ &bull; Ollama &bull; Local LLM
        </span>
        <span className="flex items-center gap-1">
          <ExternalLink className="w-3 h-3" />
          {llmConfig?.model || "gemma4:e2b"} @ {llmConfig?.url || "localhost:11434"}
        </span>
      </footer>

      {/* LLM Config Modal */}
      <LlmConfigPanel
        isOpen={showLlmConfig}
        onClose={() => {
          setShowLlmConfig(false);
          getLlmConfig().then((cfg) => setLlmConfig(cfg)).catch(() => {});
        }}
      />

      {/* Connection Modal */}
      <ConnectionModal
        isOpen={showConnectionModal}
        onClose={() => setShowConnectionModal(false)}
        connectionString={connectionString}
        onConnectionStringChange={setConnectionString}
        onConnected={handleConnected}
        onDisconnected={handleDisconnected}
        isConnected={isConnected}
      />
    </div>
  );
}

export default App;
