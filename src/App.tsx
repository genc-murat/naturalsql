import { useState, useEffect, useCallback } from "react";
import { Database, Loader2, Download, ExternalLink, Settings } from "lucide-react";
import { ThemeToggle } from "./components/ThemeToggle";
import { ConnectionPanel } from "./components/ConnectionPanel";
import { SchemaBrowser } from "./components/SchemaBrowser";
import { QueryEditor } from "./components/QueryEditor";
import { ResultsTable } from "./components/ResultsTable";
import { LlmConfigPanel } from "./components/LlmConfigPanel";
import { cacheSchema, getCachedSchema, getLlmConfig } from "./api";
import type { Schema, QueryResult, LlmConfigResponse } from "./types";
import "./App.css";

function App() {
  const [isConnected, setIsConnected] = useState(false);
  const [schema, setSchema] = useState<Schema | null>(null);
  const [isCachingSchema, setIsCachingSchema] = useState(false);
  const [cacheError, setCacheError] = useState("");
  const [queryResult, setQueryResult] = useState<QueryResult | null>(null);
  const [connectionString, setConnectionString] = useState("");
  const [showLlmConfig, setShowLlmConfig] = useState(false);
  const [llmConfig, setLlmConfig] = useState<LlmConfigResponse | null>(null);

  // Load cached schema on mount
  useEffect(() => {
    getCachedSchema().then((res) => {
      if (res.schema) {
        setSchema(res.schema);
      }
    }).catch(() => {
      // No cached schema or error loading
    });

    // Load LLM config
    getLlmConfig().then((cfg) => setLlmConfig(cfg)).catch(() => {});
  }, []);

  const handleConnected = useCallback(() => {
    setIsConnected(true);
  }, []);

  const handleDisconnected = useCallback(() => {
    setIsConnected(false);
    setQueryResult(null);
  }, []);

  const handleCacheSchema = async () => {
    if (!connectionString) return;
    
    setIsCachingSchema(true);
    setCacheError("");
    try {
      const res = await cacheSchema(connectionString);
      if (res.schema) {
        setSchema(res.schema);
      }
    } catch (err) {
      setCacheError(err instanceof Error ? err.message : "Failed to cache schema");
    } finally {
      setIsCachingSchema(false);
    }
  };

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
        <div className="flex-1">
          <ConnectionPanel
            connectionString={connectionString}
            onConnectionStringChange={setConnectionString}
            onConnected={handleConnected}
            onDisconnected={handleDisconnected}
          />
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
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar - Schema Browser */}
        <aside className="w-64 border-r border-[var(--border)] bg-[var(--bg-secondary)] flex flex-col">
          <div className="px-3 py-2 border-b border-[var(--border)]">
            <h2 className="text-sm font-semibold text-[var(--text-secondary)]">
              Schema Browser
            </h2>
          </div>
          <div className="flex-1 overflow-auto p-2">
            <SchemaBrowser schema={schema} />
          </div>
          {isConnected && !schema && (
            <div className="p-3 border-t border-[var(--border)] space-y-2">
              <button
                onClick={handleCacheSchema}
                disabled={isCachingSchema || !connectionString}
                className="w-full px-3 py-2 rounded-lg bg-[var(--accent)] text-white text-sm font-medium hover:bg-[var(--accent-hover)] disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center justify-center gap-2"
              >
                {isCachingSchema ? (
                  <>
                    <Loader2 className="w-4 h-4 animate-spin" />
                    Caching...
                  </>
                ) : (
                  <>
                    <Download className="w-4 h-4" />
                    Cache Schema
                  </>
                )}
              </button>
              {cacheError && (
                <p className="text-xs text-[var(--error)] text-center">{cacheError}</p>
              )}
            </div>
          )}
        </aside>

        {/* Main Area */}
        <main className="flex-1 flex flex-col overflow-hidden">
          {/* Query Editor */}
          <div className="p-4 border-b border-[var(--border)] bg-[var(--bg-primary)]">
            <QueryEditor onResult={handleResult} schema={schema} />
          </div>

          {/* Results */}
          <div className="flex-1 overflow-auto p-4 bg-[var(--bg-primary)]">
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
          // Reload config after closing
          getLlmConfig().then((cfg) => setLlmConfig(cfg)).catch(() => {});
        }}
      />
    </div>
  );
}

export default App;
