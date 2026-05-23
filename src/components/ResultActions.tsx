import { useState, useRef, useEffect } from "react";
import {
  Sparkles,
  BarChart3,
  MessageCircle,
  FileText,
  LayoutGrid,
  Send,
  Loader2,
  X,
  ArrowRight,
  Copy,
  Check,
  TrendingUp,
} from "lucide-react";
import { resultSetAction } from "../api";

interface ResultActionsProps {
  columns: string[];
  rows: unknown[][];
  rowCount: number;
  onApplySql: (sql: string) => void;
}

interface ActionItem {
  icon: React.ReactNode;
  label: string;
  prompt: string;
  category: "aggregate" | "visualize" | "followup" | "report" | "pivot";
}

interface ActionResult {
  id: number;
  question: string;
  response: string;
  suggestedSql: string | null;
}

const QUICK_ACTIONS: ActionItem[] = [
  { icon: <TrendingUp className="w-3.5 h-3.5" />, label: "Aggregate", prompt: "Suggest useful aggregations for this data (COUNT, SUM, AVG, GROUP BY). Provide the SQL.", category: "aggregate" },
  { icon: <BarChart3 className="w-3.5 h-3.5" />, label: "Chart", prompt: "What type of chart/visualization would best represent this data and why?", category: "visualize" },
  { icon: <LayoutGrid className="w-3.5 h-3.5" />, label: "Pivot", prompt: "Would pivoting this data make it more readable? If so, suggest how with SQL.", category: "pivot" },
  { icon: <FileText className="w-3.5 h-3.5" />, label: "Report", prompt: "Write a natural language summary report of this data.", category: "report" },
  { icon: <MessageCircle className="w-3.5 h-3.5" />, label: "Ask", prompt: "", category: "followup" },
];

export function ResultActions({ columns, rows, rowCount, onApplySql }: ResultActionsProps) {
  const [results, setResults] = useState<ActionResult[]>([]);
  const [customInput, setCustomInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [copied, setCopied] = useState(false);
  const resultId = useRef(0);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [results, isLoading]);

  const handleAction = async (action: ActionItem) => {
    if (isLoading) return;
    if (!action.prompt && action.category !== "followup") return;

    const question = action.category === "followup"
      ? customInput.trim()
      : action.prompt;

    if (!question) return;

    if (action.category === "followup") {
      setCustomInput("");
    }

    setIsLoading(true);
    try {
      const result = await resultSetAction({
        question,
        columns,
        sample_rows: rows.slice(0, 5),
        total_rows: rowCount,
      });
      setResults((prev) => [...prev, {
        id: resultId.current++,
        question,
        response: result.response,
        suggestedSql: result.suggested_sql,
      }]);
    } catch (err) {
      setResults((prev) => [...prev, {
        id: resultId.current++,
        question,
        response: err instanceof Error ? err.message : "Action failed",
        suggestedSql: null,
      }]);
    } finally {
      setIsLoading(false);
    }
  };

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  if (rowCount === 0) return null;

  return (
    <div className="space-y-3">
      {/* Quick Actions */}
      <div className="flex items-center gap-2">
        <Sparkles className="w-4 h-4 text-[var(--accent)] flex-shrink-0" />
        <span className="text-sm font-medium text-[var(--text-secondary)]">AI Actions:</span>
        <div className="flex gap-1.5">
          {QUICK_ACTIONS.map((action) => (
            <button
              key={action.label}
              onClick={() => handleAction(action)}
              disabled={isLoading || (action.category === "followup" && !customInput.trim())}
              className="px-2.5 py-1 rounded-md bg-[var(--bg-tertiary)] border border-[var(--border)] text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)] hover:border-[var(--accent)]/30 disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1"
            >
              {action.icon}
              {action.label}
            </button>
          ))}
        </div>
      </div>

      {/* Follow-up Input */}
      <div className="flex gap-2">
        <input
          type="text"
          value={customInput}
          onChange={(e) => setCustomInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleAction(QUICK_ACTIONS[4])}
          placeholder="Ask anything about this data..."
          className="flex-1 px-3 py-1.5 rounded-md bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] text-sm focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
        />
        <button
          onClick={() => handleAction(QUICK_ACTIONS[4])}
          disabled={isLoading || !customInput.trim()}
          className="px-3 py-1.5 rounded-md bg-[var(--accent)] text-white text-sm hover:bg-[var(--accent-hover)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
        >
          {isLoading ? <Loader2 className="w-4 h-4 animate-spin" /> : <Send className="w-4 h-4" />}
        </button>
      </div>

      {/* Results */}
      {results.length > 0 && (
        <div className="space-y-2">
          {results.map((r) => (
            <div key={r.id} className="p-3 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)]">
              <div className="flex items-start justify-between gap-2 mb-1.5">
                <div className="flex items-center gap-1.5">
                  <ArrowRight className="w-3 h-3 text-[var(--accent)]" />
                  <span className="text-xs font-medium text-[var(--text-muted)]">{r.question}</span>
                </div>
                <button
                  onClick={() => setResults((prev) => prev.filter((x) => x.id !== r.id))}
                  className="p-0.5 rounded hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] transition-colors"
                >
                  <X className="w-3 h-3" />
                </button>
              </div>
              <p className="text-sm text-[var(--text-secondary)] whitespace-pre-wrap leading-relaxed">
                {r.response}
              </p>

              {/* Suggested SQL */}
              {r.suggestedSql && (
                <div className="mt-2 relative">
                  <div className="px-3 py-2 rounded-md bg-[var(--bg-primary)] border border-[var(--border)] font-mono text-xs text-[var(--text-primary)] whitespace-pre-wrap pr-8">
                    {r.suggestedSql}
                  </div>
                  <div className="absolute top-2 right-2 flex gap-1">
                    <button
                      onClick={() => handleCopy(r.suggestedSql!)}
                      className="p-1 rounded hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] transition-colors"
                      title="Copy SQL"
                    >
                      {copied ? <Check className="w-3.5 h-3.5 text-[var(--success)]" /> : <Copy className="w-3.5 h-3.5" />}
                    </button>
                    <button
                      onClick={() => onApplySql(r.suggestedSql!)}
                      className="p-1 rounded bg-[var(--accent)]/20 hover:bg-[var(--accent)]/30 text-[var(--accent)] transition-colors"
                      title="Apply to editor"
                    >
                      <Send className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              )}
            </div>
          ))}
          <div ref={endRef} />
        </div>
      )}
    </div>
  );
}
