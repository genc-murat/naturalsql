import { useState, useEffect } from "react";
import {
  Loader2,
  CheckCircle,
  XCircle,
  Plug,
  Power,
  X,
  Server,
  User,
  Lock,
  Hash,
  Package,
  Bookmark,
  Trash2,
  Save,
} from "lucide-react";
import {
  connectDb,
  disconnectDb,
  listConnections,
  saveConnectionProfile,
  deleteConnectionProfile,
} from "../api";
import type { ConnectionProfileResponse } from "../types";

interface ConnectionModalProps {
  isOpen: boolean;
  onClose: () => void;
  connectionString: string;
  onConnectionStringChange: (value: string) => void;
  onConnected: () => void;
  onDisconnected: () => void;
  isConnected: boolean;
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

export function ConnectionModal({
  isOpen,
  onClose,
  connectionString,
  onConnectionStringChange,
  onConnected,
  onDisconnected,
  isConnected,
}: ConnectionModalProps) {
  const [isConnecting, setIsConnecting] = useState(false);
  const [status, setStatus] = useState<"idle" | "success" | "error">("idle");
  const [errorMessage, setErrorMessage] = useState("");
  const [savedProfiles, setSavedProfiles] = useState<ConnectionProfileResponse[]>([]);
  const [profileName, setProfileName] = useState("");
  const [savingProfile, setSavingProfile] = useState(false);
  const [saveSuccess, setSaveSuccess] = useState(false);

  const form = parseConnectionString(connectionString);
  const [localForm, setLocalForm] = useState<ConnectionForm>(form);

  // Load profiles and sync when modal opens
  useEffect(() => {
    if (isOpen) {
      setLocalForm(parseConnectionString(connectionString));
      setStatus(isConnected ? "success" : "idle");
      listConnections().then(setSavedProfiles).catch(() => {});
    }
  }, [isOpen, connectionString, isConnected]);

  const updateForm = (field: keyof ConnectionForm, value: string) => {
    const updated = { ...localForm, [field]: value };
    setLocalForm(updated);
    onConnectionStringChange(formToConnectionString(updated));
  };

  const selectProfile = (profile: ConnectionProfileResponse) => {
    setProfileName(profile.name);
    const newForm: ConnectionForm = {
      host: profile.host,
      port: profile.port,
      user: profile.user,
      password: profile.password,
      database: profile.database,
    };
    setLocalForm(newForm);
    onConnectionStringChange(formToConnectionString(newForm));
  };

  const handleDeleteProfile = async (name: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await deleteConnectionProfile(name);
      setSavedProfiles(await listConnections());
      if (profileName === name) setProfileName("");
    } catch (err) {
      console.error("Failed to delete profile:", err);
    }
  };

  const handleSaveProfile = async () => {
    if (!profileName.trim() || !localForm.host.trim()) return;
    setSavingProfile(true);
    setSaveSuccess(false);
    try {
      await saveConnectionProfile({
        name: profileName,
        ...localForm,
      });
      setSavedProfiles(await listConnections());
      setSaveSuccess(true);
      setTimeout(() => setSaveSuccess(false), 2000);
    } catch (err) {
      console.error("Failed to save profile:", err);
    } finally {
      setSavingProfile(false);
    }
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
      setTimeout(() => onClose(), 800);
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

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div
        className="w-full max-w-md rounded-xl bg-[var(--bg-primary)] border border-[var(--border)] shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
          <div className="flex items-center gap-2">
            <Server className="w-5 h-5 text-[var(--accent)]" />
            <h2 className="text-base font-semibold text-[var(--text-primary)]">MySQL Connection</h2>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] transition-colors"
          >
            <X className="w-4 h-4 text-[var(--text-muted)]" />
          </button>
        </div>

        {/* Saved Profiles */}
        {savedProfiles.length > 0 && (
          <div className="px-5 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]/50">
            <label className="text-xs font-medium text-[var(--text-muted)] flex items-center gap-1.5 mb-2">
              <Bookmark className="w-3 h-3" />
              Saved Connections
            </label>
            <div className="space-y-1">
              {savedProfiles.map((profile) => (
                <div
                  key={profile.name}
                  className={`flex items-center gap-2 px-3 py-1.5 rounded-md cursor-pointer text-sm transition-colors ${
                    profileName === profile.name
                      ? "bg-[var(--accent)]/10 text-[var(--accent)]"
                      : "hover:bg-[var(--bg-tertiary)] text-[var(--text-secondary)]"
                  }`}
                  onClick={() => selectProfile(profile)}
                >
                  <Server className="w-3.5 h-3.5 flex-shrink-0" />
                  <span className="flex-1 truncate">{profile.name}</span>
                  <span className="text-xs text-[var(--text-muted)]">
                    {profile.host}:{profile.port || "3306"}
                  </span>
                  <button
                    onClick={(e) => handleDeleteProfile(profile.name, e)}
                    className="p-0.5 rounded hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] hover:text-[var(--error)] transition-colors"
                    title="Delete profile"
                  >
                    <Trash2 className="w-3 h-3" />
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Form */}
        <form onSubmit={handleConnect} className="p-5 space-y-4">
          {/* Host & Port row */}
          <div className="flex gap-3">
            <div className="flex-1 space-y-1.5">
              <label className="text-xs font-medium text-[var(--text-muted)] flex items-center gap-1.5">
                <Server className="w-3 h-3" />
                Host
              </label>
              <input
                type="text"
                value={localForm.host}
                onChange={(e) => updateForm("host", e.target.value)}
                placeholder="localhost"
                autoFocus
                className="w-full px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] text-sm"
              />
            </div>
            <div className="w-24 space-y-1.5">
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
          <div className="space-y-1.5">
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
            <div className="flex-1 space-y-1.5">
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
            <div className="flex-1 space-y-1.5">
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

          {/* Save Profile */}
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={profileName}
              onChange={(e) => setProfileName(e.target.value)}
              placeholder="Save as profile name..."
              className="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] text-sm"
            />
            <button
              type="button"
              onClick={handleSaveProfile}
              disabled={savingProfile || !profileName.trim() || !localForm.host.trim()}
              className="px-3 py-2 rounded-lg bg-[var(--bg-tertiary)] text-[var(--text-secondary)] text-sm font-medium hover:bg-[var(--border)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
            >
              {saveSuccess ? (
                <CheckCircle className="w-3.5 h-3.5 text-[var(--success)]" />
              ) : (
                <Save className="w-3.5 h-3.5" />
              )}
              {saveSuccess ? "Saved!" : "Save"}
            </button>
          </div>

          {/* Error */}
          {status === "error" && (
            <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20 text-red-500 text-xs">
              <XCircle className="w-3.5 h-3.5 flex-shrink-0" />
              {errorMessage}
            </div>
          )}

          {/* Success */}
          {status === "success" && (
            <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-green-500/10 border border-green-500/20 text-green-500 text-xs">
              <CheckCircle className="w-3.5 h-3.5 flex-shrink-0" />
              Connected successfully
            </div>
          )}

          {/* Actions */}
          <div className="flex items-center justify-end gap-2 pt-3 border-t border-[var(--border)]">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 rounded-lg bg-[var(--bg-tertiary)] text-[var(--text-secondary)] text-sm font-medium hover:bg-[var(--border)] transition-colors"
            >
              {isConnected ? "Close" : "Cancel"}
            </button>
            {isConnected && (
              <button
                type="button"
                onClick={handleDisconnect}
                className="px-4 py-2 rounded-lg bg-[var(--bg-tertiary)] text-[var(--error)] text-sm font-medium hover:bg-[var(--border)] transition-colors flex items-center gap-1.5"
              >
                <Power className="w-3.5 h-3.5" />
                Disconnect
              </button>
            )}
            <button
              type="submit"
              disabled={isConnecting || !localForm.host.trim()}
              className="px-5 py-2 rounded-lg bg-[var(--accent)] text-white text-sm font-medium hover:bg-[var(--accent-hover)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
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
    </div>
  );
}
