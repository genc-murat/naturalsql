import { useState, useEffect } from "react";
import { Loader2, AlertTriangle, CheckCircle, Info } from "lucide-react";
import { explainSqlJson } from "../api";

interface ExplainPlanProps {
  sql: string;
}

interface ExplainNode {
  table?: string;
  access_type?: string;
  key?: string;
  key_length?: string;
  rows?: number;
  filtered?: number;
  attached_condition?: string;
  materialized?: unknown;
  children?: ExplainNode[];
  using_temporary?: boolean;
  using_filesort?: boolean;
  message?: string;
  cost_info?: { query_cost?: string };
}

function getAccessColor(type?: string): string {
  switch (type) {
    case "system":
    case "const":
      return "text-blue-400";
    case "eq_ref":
    case "ref":
      return "text-green-400";
    case "range":
    case "index":
      return "text-orange-400";
    case "ALL":
      return "text-red-400";
    default:
      return "text-[var(--text-muted)]";
  }
}

function getAccessBg(type?: string): string {
  switch (type) {
    case "system":
    case "const":
      return "bg-blue-500/10 border-blue-500/30";
    case "eq_ref":
    case "ref":
      return "bg-green-500/10 border-green-500/30";
    case "range":
    case "index":
      return "bg-orange-500/10 border-orange-500/30";
    case "ALL":
      return "bg-red-500/10 border-red-500/30";
    default:
      return "bg-[var(--bg-tertiary)] border-[var(--border)]";
  }
}

function getAccessLabel(type?: string): string {
  switch (type) {
    case "system": return "SYSTEM";
    case "const": return "CONST";
    case "eq_ref": return "EQ_REF";
    case "ref": return "REF";
    case "range": return "RANGE";
    case "index": return "INDEX";
    case "ALL": return "ALL (Full Scan)";
    default: return type || "UNKNOWN";
  }
}

function parseExplainNodes(data: unknown): ExplainNode[] {
  const nodes: ExplainNode[] = [];

  function walk(obj: unknown) {
    if (!obj || typeof obj !== "object") return;
    const o = obj as Record<string, unknown>;

    if (typeof o["table_name"] === "string" || typeof o["access_type"] === "string") {
      const node: ExplainNode = {
        table: o["table_name"] as string,
        access_type: o["access_type"] as string,
        key: o["key"] as string,
        key_length: o["key_length"] as string,
        rows: typeof o["rows"] === "number" ? o["rows"] : undefined,
        filtered: typeof o["filtered"] === "number" ? o["filtered"] : undefined,
        attached_condition: o["attached_condition"] as string,
        materialized: o["materialized"],
        using_temporary: !!o["using_temporary"],
        using_filesort: !!o["using_filesort"],
        message: o["message"] as string,
        cost_info: o["cost_info"] as ExplainNode["cost_info"],
      };

      if (o["nested_loop"]) {
        const nl = o["nested_loop"] as unknown[];
        node.children = [];
        for (const child of nl) {
          const childNodes = parseExplainNodes(child);
          node.children.push(...childNodes);
        }
      }

      if (o["order_by_subqueries"]) {
        for (const sub of o["order_by_subqueries"] as unknown[]) {
          node.children?.push(...parseExplainNodes(sub));
        }
      }

      nodes.push(node);
    }

    for (const val of Object.values(o)) {
      if (Array.isArray(val)) {
        for (const item of val) {
          walk(item);
        }
      } else if (val && typeof val === "object" && !nodes.length) {
        walk(val);
      }
    }
  }

  walk(data);
  return nodes;
}

function ExplainNodeView({ node, depth = 0 }: { node: ExplainNode; depth?: number }) {
  const issues: string[] = [];
  if (node.access_type === "ALL") issues.push("Full table scan");
  if (node.using_filesort) issues.push("Using filesort");
  if (node.using_temporary) issues.push("Using temporary table");
  if (node.filtered !== undefined && node.filtered < 100) {
    issues.push(`Only ${node.filtered}% filtered`);
  }

  return (
    <div style={{ marginLeft: depth * 24 }}>
      <div className={`rounded-lg border p-3 mb-2 ${getAccessBg(node.access_type)}`}>
        <div className="flex items-center gap-2 flex-wrap">
          {node.table && (
            <span className="font-mono font-semibold text-sm text-[var(--text-primary)]">
              {node.table}
            </span>
          )}
          <span className={`px-2 py-0.5 rounded text-xs font-bold ${getAccessColor(node.access_type)}`}>
            {getAccessLabel(node.access_type)}
          </span>
          {node.key && (
            <span className="text-xs text-[var(--text-muted)]">
              Key: <span className="font-mono text-[var(--text-secondary)]">{node.key}</span>
            </span>
          )}
          {node.rows !== undefined && (
            <span className="text-xs text-[var(--text-muted)]">
              Rows: <span className="font-mono text-[var(--text-secondary)]">{node.rows.toLocaleString()}</span>
            </span>
          )}
          {node.filtered !== undefined && (
            <span className="text-xs text-[var(--text-muted)]">
              Filtered: <span className="font-mono text-[var(--text-secondary)]">{node.filtered}%</span>
            </span>
          )}
        </div>
        {node.attached_condition && (
          <div className="mt-1.5 text-xs font-mono text-[var(--text-muted)] truncate" title={node.attached_condition}>
            WHERE: {node.attached_condition}
          </div>
        )}
        {node.message && (
          <div className="mt-1 text-xs text-[var(--accent)]">{node.message}</div>
        )}
        {issues.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1">
            {issues.map((issue, i) => (
              <span key={i} className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-xs bg-red-500/10 text-red-400">
                <AlertTriangle className="w-3 h-3" />
                {issue}
              </span>
            ))}
          </div>
        )}
      </div>
      {node.children?.map((child, i) => (
        <ExplainNodeView key={i} node={child} depth={depth + 1} />
      ))}
    </div>
  );
}

export function ExplainPlan({ sql }: ExplainPlanProps) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [nodes, setNodes] = useState<ExplainNode[]>([]);
  const [rawJson, setRawJson] = useState("");
  const [showRaw, setShowRaw] = useState(false);
  const [costInfo, setCostInfo] = useState<string>("");

  useEffect(() => {
    if (!sql.trim()) return;
    setLoading(true);
    setError("");
    explainSqlJson(sql)
      .then((jsonStr) => {
        setRawJson(jsonStr);
        try {
          const parsed = JSON.parse(jsonStr);
          const n = parseExplainNodes(parsed);
          setNodes(n);
          if (parsed.query_block?.cost_info?.query_cost) {
            setCostInfo(parsed.query_block.cost_info.query_cost);
          }
        } catch {
          setError("Failed to parse EXPLAIN JSON");
        }
      })
      .catch((err) => setError(err instanceof Error ? err.message : String(err)))
      .finally(() => setLoading(false));
  }, [sql]);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="w-5 h-5 animate-spin text-[var(--accent)]" />
        <span className="ml-2 text-sm text-[var(--text-muted)]">Running EXPLAIN...</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-4 text-center">
        <AlertTriangle className="w-5 h-5 mx-auto mb-2 text-[var(--error)]" />
        <p className="text-sm text-[var(--error)]">{error}</p>
      </div>
    );
  }

  if (nodes.length === 0 && !rawJson) {
    return (
      <div className="p-4 text-center text-sm text-[var(--text-muted)]">
        Click the EXPLAIN button to analyze query plan
      </div>
    );
  }

  const warnings = nodes.reduce<string[]>((acc, n) => {
    function walk(node: ExplainNode) {
      if (node.access_type === "ALL") acc.push(`Table "${node.table}" uses full scan`);
      if (node.using_filesort) acc.push(`Table "${node.table}" uses filesort`);
      if (node.using_temporary) acc.push(`Table "${node.table}" uses temporary table`);
      node.children?.forEach(walk);
    }
    walk(n);
    return acc;
  }, []);

  return (
    <div className="space-y-3">
      {/* Summary */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {warnings.length === 0 ? (
            <CheckCircle className="w-4 h-4 text-green-400" />
          ) : (
            <AlertTriangle className="w-4 h-4 text-orange-400" />
          )}
          <span className="text-sm text-[var(--text-secondary)]">
            {warnings.length === 0 ? "No issues detected" : `${warnings.length} issue${warnings.length > 1 ? "s" : ""} found`}
          </span>
          {costInfo && (
            <span className="text-xs text-[var(--text-muted)] ml-2">
              Estimated cost: {costInfo}
            </span>
          )}
        </div>
        <button
          onClick={() => setShowRaw(!showRaw)}
          className="flex items-center gap-1 px-2 py-1 rounded text-xs text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)]"
        >
          <Info className="w-3 h-3" />
          {showRaw ? "Hide JSON" : "Show JSON"}
        </button>
      </div>

      {/* Visual Tree */}
      {!showRaw && (
        <div>
          {nodes.map((node, i) => (
            <ExplainNodeView key={i} node={node} />
          ))}
        </div>
      )}

      {/* Raw JSON */}
      {showRaw && (
        <pre className="p-3 rounded-md bg-[var(--bg-tertiary)] border border-[var(--border)] text-xs font-mono text-[var(--text-secondary)] overflow-auto max-h-96 whitespace-pre-wrap">
          {JSON.stringify(JSON.parse(rawJson), null, 2)}
        </pre>
      )}

      {/* Legend */}
      <div className="flex flex-wrap gap-3 pt-2 border-t border-[var(--border)]">
        <span className="text-xs text-[var(--text-muted)]">Legend:</span>
        {[
          { label: "Const/System", color: "text-blue-400" },
          { label: "Ref/Eq_ref", color: "text-green-400" },
          { label: "Range/Index", color: "text-orange-400" },
          { label: "ALL (Scan)", color: "text-red-400" },
        ].map((item) => (
          <span key={item.label} className={`text-xs font-medium ${item.color}`}>
            {item.label}
          </span>
        ))}
      </div>
    </div>
  );
}
