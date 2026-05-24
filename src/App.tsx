import { useState, useEffect, useCallback, useRef } from "react";
import { Database, ExternalLink, Settings, CheckCircle, Plug } from "lucide-react";
import { ThemeToggle } from "./components/ThemeToggle";
import { ConnectionModal } from "./components/ConnectionModal";
import { SchemaBrowser } from "./components/SchemaBrowser";
import { QueryEditor } from "./components/QueryEditor";
import { ResultVisualization } from "./components/ResultVisualization";
import { AnalysisChat } from "./components/AnalysisChat";
import { LlmConfigPanel } from "./components/LlmConfigPanel";
import { TableStructure } from "./components/TableStructure";
import { QueryTabs } from "./components/QueryTabs";
import {
  listDatabases,
  cacheSchema,
  getCachedSchema,
  listCachedDatabases,
  removeCachedSchema,
  getLlmConfig,
  executeSql,
} from "./api";
import type { Schema, QueryResult, LlmConfigResponse, QueryTab } from "./types";
import "./App.css";

let tabCounter = 1;

function createTab(): QueryTab {
  const num = tabCounter++;
  return {
    id: crypto.randomUUID(),
    name: `Query ${num}`,
    sql: "",
    naturalLanguage: "",
    result: null,
    toolSteps: [],
    toolIterations: 0,
    toolFallback: false,
  };
}

function App() {
  const [databases, setDatabases] = useState<string[]>([]);
  const [cachedDatabases, setCachedDatabases] = useState<string[]>([]);
  const [selectedDatabase, setSelectedDatabase] = useState<string | null>(null);
  const [schema, setSchema] = useState<Schema | null>(null);
  const [isCaching, setIsCaching] = useState(false);
  const [cachingDatabase, setCachingDatabase] = useState<string | null>(null);
  const [cacheError, setCacheError] = useState("");
  const [connectionString, setConnectionString] = useState("");
  const [showLlmConfig, setShowLlmConfig] = useState(false);
  const [showAnalysisChat, setShowAnalysisChat] = useState(false);
  const [showConnectionModal, setShowConnectionModal] = useState(false);
  const [llmConfig, setLlmConfig] = useState<LlmConfigResponse | null>(null);
  const [isConnected, setIsConnected] = useState(false);
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState(288);
  const [isSidebarDragging, setIsSidebarDragging] = useState(false);
  const [isEditorCollapsed, setIsEditorCollapsed] = useState(false);
  const [tableStructureTarget, setTableStructureTarget] = useState<{ database: string; table: string } | null>(null);

  const [tabs, setTabs] = useState<QueryTab[]>(() => {
    const saved = localStorage.getItem("naturalsql-session");
    if (saved) {
      try {
        const session = JSON.parse(saved);
        if (session.tabs && session.tabs.length > 0 && Date.now() - session.lastSaved < 86400000) {
          tabCounter = session.tabs.length + 1;
          return session.tabs.map((t: QueryTab) => ({
            ...t,
            result: null,
            toolSteps: [],
            toolIterations: 0,
            toolFallback: false,
          }));
        }
      } catch {}
    }
    return [createTab()];
  });
  const [activeTabId, setActiveTabId] = useState(tabs[0].id);
  const mainAreaRef = useRef<HTMLDivElement>(null);

  const activeTab = tabs.find((t) => t.id === activeTabId) || tabs[0];

  const updateTab = useCallback((id: string, updates: Partial<QueryTab>) => {
    setTabs((prev) => prev.map((t) => (t.id === id ? { ...t, ...updates } : t)));
  }, []);

  // Session save (debounced)
  useEffect(() => {
    const timeout = setTimeout(() => {
      const session = {
        tabs: tabs.map((t) => ({ id: t.id, name: t.name, sql: t.sql, naturalLanguage: t.naturalLanguage })),
        activeTabId,
        selectedDatabase,
        sidebarWidth,
        isSidebarCollapsed,
        lastSaved: Date.now(),
      };
      localStorage.setItem("naturalsql-session", JSON.stringify(session));
    }, 2000);
    return () => clearTimeout(timeout);
  }, [tabs, activeTabId, selectedDatabase, sidebarWidth, isSidebarCollapsed]);

  // Restore selected database
  useEffect(() => {
    const saved = localStorage.getItem("naturalsql-session");
    if (saved) {
      try {
        const session = JSON.parse(saved);
        if (session.selectedDatabase && Date.now() - session.lastSaved < 86400000) {
          setSelectedDatabase(session.selectedDatabase);
          getCachedSchema(session.selectedDatabase).then((res) => {
            if (res.schema) setSchema(res.schema);
          }).catch(() => {});
        }
      } catch {}
    }
  }, []);

  // Keyboard shortcuts for tabs
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "t") {
        e.preventDefault();
        handleNewTab();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === "w") {
        e.preventDefault();
        if (tabs.length > 1) handleCloseTab(activeTabId);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [tabs, activeTabId]);

  // Load LLM config and cached databases on mount
  useEffect(() => {
    getLlmConfig().then((cfg) => setLlmConfig(cfg)).catch(() => {});
    listCachedDatabases().then((dbs) => setCachedDatabases(dbs)).catch(() => {});
  }, []);

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
    setTabs((prev) => prev.map((t) => ({ ...t, result: null })));
    setSchema(null);
    setSelectedDatabase(null);
    setCacheError("");

    try {
      const dbs = await listDatabases();
      setDatabases(dbs);

      const parsed = parseDatabaseFromUrl(connectionString);
      if (parsed && dbs.includes(parsed)) {
        setSelectedDatabase(parsed);
        try {
          const res = await getCachedSchema(parsed);
          if (res.schema) {
            setSchema(res.schema);
          }
        } catch {}
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
    setTabs((prev) => prev.map((t) => ({ ...t, result: null })));
    setCacheError("");
  }, []);

  const handleSelectDatabase = useCallback(async (db: string) => {
    setSelectedDatabase(db);
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
    updateTab(activeTabId, { result });
  }, [activeTabId, updateTab]);

  const handleViewData = useCallback(async (database: string, table: string) => {
    const sql = `SELECT * FROM \`${database}\`.\`${table}\` LIMIT 100`;
    try {
      const result = await executeSql({ sql });
      const newTab = createTab();
      newTab.sql = sql;
      newTab.name = `${table}`;
      newTab.result = result;
      setTabs((prev) => [...prev, newTab]);
      setActiveTabId(newTab.id);
    } catch {}
  }, []);

  const handleNewTab = () => {
    const newTab = createTab();
    setTabs((prev) => [...prev, newTab]);
    setActiveTabId(newTab.id);
  };

  const handleCloseTab = (id: string) => {
    setTabs((prev) => {
      if (prev.length <= 1) return prev;
      const idx = prev.findIndex((t) => t.id === id);
      const next = prev.filter((t) => t.id !== id);
      if (id === activeTabId) {
        const newIdx = Math.min(idx, next.length - 1);
        setActiveTabId(next[newIdx].id);
      }
      return next;
    });
  };

  const handleApplySql = useCallback((sql: string) => {
    updateTab(activeTabId, { sql });
    setIsEditorCollapsed(false);
  }, [activeTabId, updateTab]);

  return (
    <div className="h-screen flex flex-col bg-[var(--bg-primary)]">
      {/* Header */}
      <header className="flex items-center justify-between px-4 py-2.5 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2">
            <Database className="w-5 h-5 text-[var(--accent)]" />
            <h1 className="text-base font-bold text-[var(--text-primary)]">
              Natural<span className="text-[var(--accent)]">SQL</span>
            </h1>
          </div>
          <div className="h-5 w-px bg-[var(--border)]" />
          <button
            onClick={() => setShowConnectionModal(true)}
            className={`flex items-center gap-2 px-3 py-1.5 rounded-md text-sm transition-colors ${
              isConnected
                ? "text-[var(--success)] hover:bg-[var(--success)]/10"
                : "text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)] hover:text-[var(--text-secondary)]"
            }`}
          >
            {isConnected ? (
              <>
                <CheckCircle className="w-3.5 h-3.5" />
                <span>{parseDatabaseFromUrl(connectionString) || "Connected"}</span>
              </>
            ) : (
              <>
                <Plug className="w-3.5 h-3.5" />
                <span>New Connection</span>
              </>
            )}
          </button>
        </div>

        <div className="flex items-center gap-1">
          <AnalysisChat
            isOpen={showAnalysisChat}
            onToggle={() => setShowAnalysisChat(!showAnalysisChat)}
          />
          <button
            onClick={() => setShowLlmConfig(true)}
            className="p-2 rounded-md transition-colors hover:bg-[var(--bg-tertiary)]"
            title="LLM Settings"
          >
            <Settings className="w-4 h-4 text-[var(--text-muted)]" />
          </button>
          <ThemeToggle />
        </div>
      </header>

      {/* Main Content */}
      <div ref={mainAreaRef} className={`flex flex-1 overflow-hidden transition-all duration-300 ${showAnalysisChat ? "mr-96" : ""}`}>
        {/* Sidebar - Schema Browser */}
        {!isSidebarCollapsed && (
          <>
            <aside
              className="border-r border-[var(--border)] bg-[var(--bg-secondary)] flex flex-col shrink-0 transition-all duration-200"
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
                  onViewData={handleViewData}
                  onViewStructure={(db, tbl) => setTableStructureTarget({ database: db, table: tbl })}
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

            <div
              className="w-1.5 cursor-col-resize group flex flex-col items-center justify-center shrink-0 bg-[var(--border)] hover:bg-[var(--accent)]/30 transition-colors"
              onMouseDown={() => setIsSidebarDragging(true)}
            >
              <div className="h-16 w-1 rounded-full bg-[var(--text-muted)]/40 group-hover:bg-[var(--accent)] transition-colors" />
            </div>
          </>
        )}

        {/* Main Area */}
        <main className="flex-1 flex flex-col overflow-hidden">
          {/* Tab Bar */}
          <QueryTabs
            tabs={tabs}
            activeTabId={activeTabId}
            onSelectTab={setActiveTabId}
            onCloseTab={handleCloseTab}
            onNewTab={handleNewTab}
            onRenameTab={(id, name) => updateTab(id, { name })}
          />

          {/* Editor + Results Split */}
          {!isEditorCollapsed ? (
            <div className="flex flex-1 overflow-hidden">
              <div className="flex flex-col h-full border-r border-[var(--border)] shrink-0" style={{ width: "42%" }}>
                <QueryEditor
                  key={activeTabId}
                  onResult={handleResult}
                  schema={schema}
                  selectedDatabase={selectedDatabase}
                  tableNames={schema?.tables.map((t) => t.name) || []}
                  initialSql={activeTab.sql}
                  initialNaturalLanguage={activeTab.naturalLanguage}
                  onSqlChange={(sql) => updateTab(activeTabId, { sql })}
                  onNlChange={(nl) => updateTab(activeTabId, { naturalLanguage: nl })}
                  onToolSteps={(steps, iters, fallback) => updateTab(activeTabId, { toolSteps: steps, toolIterations: iters, toolFallback: fallback })}
                  onCollapse={() => setIsEditorCollapsed(true)}
                />
              </div>

              <div className="flex-1 min-w-0 overflow-hidden bg-[var(--bg-primary)]">
                <ResultVisualization
                  result={activeTab.result}
                  onApplySql={handleApplySql}
                  currentSql={activeTab.sql}
                />
              </div>
            </div>
          ) : (
            <div className="flex-1 min-h-0 overflow-hidden bg-[var(--bg-primary)]">
              <ResultVisualization
                result={activeTab.result}
                onApplySql={handleApplySql}
                currentSql={activeTab.sql}
              />
            </div>
          )}
        </main>
      </div>

      {/* Collapsible Sidebar Toggle */}
      <button
        onClick={() => setIsSidebarCollapsed(!isSidebarCollapsed)}
        className="fixed left-0 top-1/2 -translate-y-1/2 z-30 p-1 rounded-r-md bg-[var(--bg-secondary)] border border-l-0 border-[var(--border)] text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[var(--bg-tertiary)] transition-colors"
        style={{ left: isSidebarCollapsed ? 0 : sidebarWidth + 6 }}
        title={isSidebarCollapsed ? "Show sidebar" : "Hide sidebar"}
      >
        <ExternalLink className="w-3 h-3" />
      </button>

      {isEditorCollapsed && (
        <button
          onClick={() => setIsEditorCollapsed(false)}
          className="fixed bottom-14 left-1/2 -translate-x-1/2 z-30 px-4 py-1.5 rounded-t-md bg-[var(--bg-secondary)] border border-b-0 border-[var(--border)] text-xs text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[var(--bg-tertiary)] transition-colors flex items-center gap-1.5"
          title="Show editor"
        >
          <Database className="w-3 h-3" />
          Show Editor
        </button>
      )}

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

      <LlmConfigPanel
        isOpen={showLlmConfig}
        onClose={() => {
          setShowLlmConfig(false);
          getLlmConfig().then((cfg) => setLlmConfig(cfg)).catch(() => {});
        }}
      />

      <ConnectionModal
        isOpen={showConnectionModal}
        onClose={() => setShowConnectionModal(false)}
        connectionString={connectionString}
        onConnectionStringChange={setConnectionString}
        onConnected={handleConnected}
        onDisconnected={handleDisconnected}
        isConnected={isConnected}
      />

      {tableStructureTarget && (
        <TableStructure
          isOpen={!!tableStructureTarget}
          onClose={() => setTableStructureTarget(null)}
          database={tableStructureTarget.database}
          table={tableStructureTarget.table}
        />
      )}
    </div>
  );
}

export default App;
