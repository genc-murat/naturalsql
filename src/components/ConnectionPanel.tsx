import { useState, useRef, useEffect } from "react";
import {
  Database,
  Loader2,
  CheckCircle,
  XCircle,
  Plug,
  Power,
  ChevronDown,
  ChevronUp,
  Server,
  User,
  Lock,
  Hash,
  Package,
} from "lucide-react";
import { connectDb, disconnectDb } from "../api";

interface ConnectionPanelProps {
  connectionString: string;
  onConnectionStringChange: (value: string) => void;
  onConnected: () => void;
  onDisconnected: () => void;
}

interface ConnectionForm {
  host: string;
  port: string;
  user: string;
  password: string;
  database: string;
}

function formToConnectionString(form: ConnectionForm): string {
  const { host, port, user, password, database } = form;
  if (!host) return "";
  const pw = password ? `:${encodeURIComponent(password)}` : "";
  return `mysql://${user}${pw}@${host}:${port || "3306"}/${database}`;
}

function parseConnectionString(str: string): ConnectionForm {
  // mysql://user:pass@host:port/db
  try {
    const afterProtocol = str.replace(/^mysql:\/\//, "");
    const [authHostPort, ...dbParts] = afterProtocol.split("/");
    const database = dbParts.join("/").split("?")[0] || "";

    const [auth, hostPort] = authHostPort.split("@");
    const [user, ...passParts] = auth.split(":");
    const password = passParts.join(":");

    const [host, port] = hostPort ? hostPort.split(":") : ["", ""];

    return {
      host: host || "",
      port: port || "3306",
      user: decodeURIComponent(user || ""),
      password: decodeURIComponent(password || ""),
      database: decodeURIComponent(database || ""),
    };
  } catch {
    return { host: "", port: "3306", user: "", password: "", database: "" };
  }
}

export function ConnectionPanel({
  connectionString,
  onConnectionStringChange,
  onConnected,
  onDisconnected,
}: ConnectionPanelProps) {
  const [isConnecting, setIsConnecting] = useState(false);
  const [status, setStatus] = useState<"idle" | "success" | "error">("idle");
  const [errorMessage, setErrorMessage] = useState("");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);

  const form = parseConnectionString(connectionString);
  const [localForm, setLocalForm] = useState<ConnectionForm>(form);

  // Sync from parent when connection string changes externally
  useEffect(() => {
    const parsed = parseConnectionString(connectionString);
    setLocalForm(parsed);
  }, [connectionString]);

  const updateForm = (field: keyof ConnectionForm, value: string) => {
    const updated = { ...localForm, [field]: value };
    setLocalForm(updated);
    onConnectionStringChange(formToConnectionString(updated));
  };

  const handleConnect = async (e: React.FormEvent) => {
    e.preventDefault();
    const connStr = formToConnectionString(localForm);
    if (!connStr || !localForm.host.trim()) return;

    setIsConnecting(true);
    setStatus("idle");
    setErrorMessage("");

    try {
      await connectDb(connStr);
      setStatus("success");
      onConnected();
    } catch (err) {
      setStatus("error");
      setErrorMessage(err instanceof Error ? err.message : "Connection failed");
    } finally {
      setIsConnecting(false);
    }
  };

  const handleDisconnect = async () => {
    try {
      await disconnectDb();
      setStatus("idle");
      onDisconnected();
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : "Disconnect failed");
    }
  };

  const isConnected = status === "success";

  return (
    <div ref={panelRef} className="relative">
      {/* Connection bar */}
      <div className="flex items-center gap-2">
        <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)]">
          <Database className={`w-4 h-4 ${isConnected ? "text-[var(--success)]" : "text-[var(--text-muted)]"}`} />
          <span className="text-sm text-[var(--text-secondary)]">
            {isConnected
              ? localForm.database || localForm.host
              : localForm.host
                ? `${localForm.host}:${localForm.port || "3306"}`
                : "Not connected"}
          </span>
          {isConnected && <CheckCircle className="w-3.5 h-3.5 text-[var(--success)]" />}
        </div>

        <button
          onClick={() => setShowAdvanced(!showAdvanced)}
          className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] transition-colors"
          title={showAdvanced ? "Hide connection form" : "Edit connection"}
        >
          {showAdvanced ? (
            <ChevronUp className="w-4 h-4 text-[var(--text-muted)]" />
          ) : (
            <ChevronDown className="w-4 h-4 text-[var(--text-muted)]" />
          )}
        </button>

        {isConnected ? (
          <button
            onClick={handleDisconnect}
            className="px-3 py-1.5 rounded-md bg-[var(--bg-tertiary)] text-[var(--text-secondary)] text-sm font-medium hover:bg-[var(--border)] transition-colors flex items-center gap-1.5"
          >
            <Power className="w-3.5 h-3.5" />
            Disconnect
          </button>
        ) : (
          <button
            onClick={handleConnect}
            disabled={isConnecting || !localForm.host.trim()}
            className="px-3 py-1.5 rounded-md bg-[var(--accent)] text-white text-sm font-medium hover:bg-[var(--accent-hover)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
          >
            {isConnecting ? (
              <>
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                Connecting...
              </>
            ) : (
              <>
                <Plug className="w-3.5 h-3.5" />
                Connect
              </>
            )}
          </button>
        )}
      </div>

      {/* Expandable connection form */}
      {showAdvanced && (
        <div className="absolute top-full right-0 mt-2 z-50 w-[480px] rounded-xl bg-[var(--bg-primary)] border border-[var(--border)] shadow-2xl overflow-hidden">
          {/* Header */}
          <div className="px-4 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
            <h3 className="text-sm font-semibold text-[var(--text-primary)]">MySQL Connection</h3>
          </div>

          {/* Form */}
          <form onSubmit={handleConnect} className="p-4 space-y-3">
            {/* Host & Port row */}
            <div className="flex gap-3">
              <div className="flex-1 space-y-1">
                <label className="text-xs font-medium text-[var(--text-muted)] flex items-center gap-1.5">
                  <Server className="w-3 h-3" />
                  Host
                </label>
                <input
                  type="text"
                  value={localForm.host}
                  onChange={(e) => updateForm("host", e.target.value)}
                  placeholder="localhost"
                  className="w-full px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] text-sm"
                />
              </div>
              <div className="w-24 space-y-1">
                <label className="text-xs font-medium text-[var(--text-muted)] flex items-center gap-1.5">
                  <Hash className="w-3 h-3" />
                  Port
                </label>
                <input
                  type="text"
                  value={localForm.port}
                  onChange={(e) => updateForm("port", e.target.value)}
                  placeholder="3306"
                  className="w-full px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] text-sm"
                />
              </div>
            </div>

            {/* Database */}
            <div className="space-y-1">
              <label className="text-xs font-medium text-[var(--text-muted)] flex items-center gap-1.5">
                <Package className="w-3 h-3" />
                Database <span className="text-[var(--text-muted)]">(optional)</span>
              </label>
              <input
                type="text"
                value={localForm.database}
                onChange={(e) => updateForm("database", e.target.value)}
                placeholder="my_database"
                className="w-full px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] text-sm"
              />
            </div>

            {/* User & Password row */}
            <div className="flex gap-3">
              <div className="flex-1 space-y-1">
                <label className="text-xs font-medium text-[var(--text-muted)] flex items-center gap-1.5">
                  <User className="w-3 h-3" />
                  Username
                </label>
                <input
                  type="text"
                  value={localForm.user}
                  onChange={(e) => updateForm("user", e.target.value)}
                  placeholder="root"
                  className="w-full px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] text-sm"
                />
              </div>
              <div className="flex-1 space-y-1">
                <label className="text-xs font-medium text-[var(--text-muted)] flex items-center gap-1.5">
                  <Lock className="w-3 h-3" />
                  Password
                </label>
                <input
                  type="password"
                  value={localForm.password}
                  onChange={(e) => updateForm("password", e.target.value)}
                  placeholder="••••••••"
                  className="w-full px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] text-sm"
                />
              </div>
            </div>

            {/* Error */}
            {status === "error" && (
              <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20 text-red-500 text-xs">
                <XCircle className="w-3.5 h-3.5 flex-shrink-0" />
                {errorMessage}
              </div>
            )}

            {/* Actions */}
            <div className="flex items-center justify-end gap-2 pt-2 border-t border-[var(--border)]">
              {isConnected && (
                <button
                  type="button"
                  onClick={handleDisconnect}
                  className="px-4 py-2 rounded-lg bg-[var(--bg-tertiary)] text-[var(--text-secondary)] text-sm font-medium hover:bg-[var(--border)] transition-colors flex items-center gap-1.5"
                >
                  <Power className="w-3.5 h-3.5" />
                  Disconnect
                </button>
              )}
              <button
                type="submit"
                disabled={isConnecting || !localForm.host.trim()}
                className="px-4 py-2 rounded-lg bg-[var(--accent)] text-white text-sm font-medium hover:bg-[var(--accent-hover)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
              >
                {isConnecting ? (
                  <>
                    <Loader2 className="w-3.5 h-3.5 animate-spin" />
                    Connecting...
                  </>
                ) : (
                  <>
                    <Plug className="w-3.5 h-3.5" />
                    {isConnected ? "Reconnect" : "Connect"}
                  </>
                )}
              </button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
}
