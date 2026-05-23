import { useState } from "react";
import { Database, Loader2, CheckCircle, XCircle } from "lucide-react";
import { connectDb, disconnectDb } from "../api";

interface ConnectionPanelProps {
  connectionString: string;
  onConnectionStringChange: (value: string) => void;
  onConnected: () => void;
  onDisconnected: () => void;
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

  const handleConnect = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!connectionString.trim()) return;

    setIsConnecting(true);
    setStatus("idle");
    setErrorMessage("");

    try {
      await connectDb(connectionString);
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

  return (
    <div className="flex items-center gap-3">
      <form onSubmit={handleConnect} className="flex items-center gap-2 flex-1">
        <Database className="w-5 h-5 text-[var(--text-muted)] flex-shrink-0" />
        <input
          type="text"
          value={connectionString}
          onChange={(e) => onConnectionStringChange(e.target.value)}
          placeholder="mysql://user:password@localhost:3306/database"
          className="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
        />
        <button
          type="submit"
          disabled={isConnecting || !connectionString.trim()}
          className="px-4 py-2 rounded-lg bg-[var(--accent)] text-white font-medium hover:bg-[var(--accent-hover)] disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center gap-2"
        >
          {isConnecting ? (
            <>
              <Loader2 className="w-4 h-4 animate-spin" />
              Connecting...
            </>
          ) : (
            "Connect"
          )}
        </button>
        <button
          type="button"
          onClick={handleDisconnect}
          className="px-4 py-2 rounded-lg bg-[var(--bg-tertiary)] text-[var(--text-secondary)] font-medium hover:bg-[var(--border)] transition-colors"
        >
          Disconnect
        </button>
      </form>
      {status === "success" && (
        <CheckCircle className="w-5 h-5 text-[var(--success)]" />
      )}
      {status === "error" && (
        <div className="flex items-center gap-2">
          <XCircle className="w-5 h-5 text-[var(--error)]" />
          <span className="text-[var(--error)] text-sm">{errorMessage}</span>
        </div>
      )}
    </div>
  );
}
