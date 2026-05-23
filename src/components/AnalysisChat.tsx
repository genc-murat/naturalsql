import { useState, useRef, useEffect, useCallback } from "react";
import {
  MessageSquare,
  Send,
  Loader2,
  ChevronRight,
  Code,
  Database,
} from "lucide-react";
import { analyzeData } from "../api";

interface ChatMessage {
  id: number;
  role: "user" | "assistant";
  content: string;
  sql?: string;
  data?: { columns: string[]; rows: unknown[][]; row_count: number } | null;
}

interface AnalysisChatProps {
  isOpen: boolean;
  onToggle: () => void;
}

export function AnalysisChat({ isOpen, onToggle }: AnalysisChatProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const msgId = useRef(0);

  const scrollToBottom = useCallback(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  useEffect(() => {
    scrollToBottom();
  }, [messages, isLoading]);

  const handleSend = async () => {
    if (!input.trim() || isLoading) return;

    const userMsg: ChatMessage = {
      id: msgId.current++,
      role: "user",
      content: input.trim(),
    };
    setMessages((prev) => [...prev, userMsg]);
    const question = input.trim();
    setInput("");
    setIsLoading(true);

    try {
      const result = await analyzeData({ question });
      const assistantMsg: ChatMessage = {
        id: msgId.current++,
        role: "assistant",
        content: result.answer,
        sql: result.sql,
        data: result.data,
      };
      setMessages((prev) => [...prev, assistantMsg]);
    } catch (err) {
      const errorMsg: ChatMessage = {
        id: msgId.current++,
        role: "assistant",
        content: err instanceof Error ? err.message : "Analysis failed. Please try again.",
      };
      setMessages((prev) => [...prev, errorMsg]);
    } finally {
      setIsLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <>
      {/* Toggle Button */}
      <button
        onClick={onToggle}
        className="p-2 rounded-md transition-colors hover:bg-[var(--bg-tertiary)]"
        title="Data Analysis Chat"
      >
        <MessageSquare className="w-4 h-4 text-[var(--text-muted)]" />
      </button>

      {/* Chat Panel */}
      {isOpen && (
        <div className="fixed right-0 top-0 bottom-0 z-40 w-96 bg-[var(--bg-primary)] border-l border-[var(--border)] flex flex-col shadow-2xl">
          {/* Header */}
          <div className="flex items-center justify-between px-4 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
            <div className="flex items-center gap-2">
              <Database className="w-4 h-4 text-[var(--accent)]" />
              <h2 className="text-sm font-semibold text-[var(--text-primary)]">Data Analysis</h2>
            </div>
            <button
              onClick={onToggle}
              className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] transition-colors"
            >
              <ChevronRight className="w-4 h-4 text-[var(--text-muted)]" />
            </button>
          </div>

          {/* Messages */}
          <div className="flex-1 overflow-auto p-4 space-y-4">
            {messages.length === 0 && (
              <div className="text-center py-8">
                <Database className="w-8 h-8 text-[var(--text-muted)] mx-auto mb-3" />
                <p className="text-sm text-[var(--text-secondary)] mb-2">
                  Ask questions about your data
                </p>
                <p className="text-xs text-[var(--text-muted)]">
                  LLM will generate SQL, execute it, and interpret the results
                </p>
                <div className="mt-4 space-y-1.5 text-left">
                  <button
                    onClick={() => setInput("How many records are in each table?")}
                    className="block w-full text-left px-3 py-1.5 rounded-md bg-[var(--bg-secondary)] border border-[var(--border)] text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-tertiary)] transition-colors"
                  >
                    "How many records are in each table?"
                  </button>
                  <button
                    onClick={() => setInput("Show me the top 5 most recent entries")}
                    className="block w-full text-left px-3 py-1.5 rounded-md bg-[var(--bg-secondary)] border border-[var(--border)] text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-tertiary)] transition-colors"
                  >
                    "Show me the top 5 most recent entries"
                  </button>
                  <button
                    onClick={() => setInput("What are the most common values in the database?")}
                    className="block w-full text-left px-3 py-1.5 rounded-md bg-[var(--bg-secondary)] border border-[var(--border)] text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-tertiary)] transition-colors"
                  >
                    "What are the most common values?"
                  </button>
                </div>
              </div>
            )}

            {messages.map((msg) => (
              <div
                key={msg.id}
                className={`flex flex-col ${
                  msg.role === "user" ? "items-end" : "items-start"
                }`}
              >
                <div
                  className={`max-w-[90%] px-3 py-2 rounded-lg text-sm ${
                    msg.role === "user"
                      ? "bg-[var(--accent)] text-white rounded-br-sm"
                      : "bg-[var(--bg-secondary)] text-[var(--text-primary)] rounded-bl-sm border border-[var(--border)]"
                  }`}
                >
                  <p className="whitespace-pre-wrap leading-relaxed">{msg.content}</p>
                </div>

                {/* SQL snippet */}
                {msg.sql && (
                  <details className="mt-1 max-w-[90%] w-full">
                    <summary className="flex items-center gap-1.5 px-2 py-1 rounded-md bg-[var(--bg-secondary)] border border-[var(--border)] text-xs text-[var(--text-muted)] cursor-pointer hover:bg-[var(--bg-tertiary)] transition-colors">
                      <Code className="w-3 h-3" />
                      Generated SQL
                    </summary>
                    <div className="mt-1 px-2 py-1.5 rounded-md bg-[var(--bg-secondary)] border border-[var(--border)] font-mono text-xs text-[var(--text-secondary)] whitespace-pre-wrap">
                      {msg.sql}
                    </div>
                  </details>
                )}

                {/* Data rows summary */}
                {msg.data && msg.data.row_count > 0 && (
                  <details className="mt-1 max-w-[90%] w-full">
                    <summary className="flex items-center gap-1.5 px-2 py-1 rounded-md bg-[var(--bg-secondary)] border border-[var(--border)] text-xs text-[var(--text-muted)] cursor-pointer hover:bg-[var(--bg-tertiary)] transition-colors">
                      <Database className="w-3 h-3" />
                      {msg.data.row_count} row{msg.data.row_count !== 1 ? "s" : ""} returned
                    </summary>
                    <div className="mt-1 overflow-auto max-h-32 px-2 py-1.5 rounded-md bg-[var(--bg-secondary)] border border-[var(--border)]">
                      <table className="w-full text-xs font-mono">
                        <thead>
                          <tr className="text-[var(--text-muted)] border-b border-[var(--border)]">
                            {msg.data.columns.map((col) => (
                              <th key={col} className="px-2 py-1 text-left">{col}</th>
                            ))}
                          </tr>
                        </thead>
                        <tbody>
                          {msg.data.rows.slice(0, 10).map((row, i) => (
                            <tr key={i} className="text-[var(--text-secondary)] border-b border-[var(--border)]">
                              {row.map((cell, j) => (
                                <td key={j} className="px-2 py-1">{cell === null ? "NULL" : String(cell)}</td>
                              ))}
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </details>
                )}
              </div>
            ))}

            {isLoading && (
              <div className="flex items-center gap-2 text-[var(--text-muted)] text-sm">
                <Loader2 className="w-4 h-4 animate-spin" />
                Analyzing...
              </div>
            )}
            <div ref={messagesEndRef} />
          </div>

          {/* Input */}
          <div className="px-3 py-3 border-t border-[var(--border)] bg-[var(--bg-secondary)]">
            <div className="flex gap-2">
              <input
                type="text"
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder="Ask about your data..."
                className="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-primary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] text-sm focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
              />
              <button
                onClick={handleSend}
                disabled={isLoading || !input.trim()}
                className="px-3 py-2 rounded-lg bg-[var(--accent)] text-white hover:bg-[var(--accent-hover)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
              >
                {isLoading ? <Loader2 className="w-4 h-4 animate-spin" /> : <Send className="w-4 h-4" />}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
