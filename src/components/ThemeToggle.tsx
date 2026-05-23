import { Sun, Moon } from "lucide-react";
import { useTheme } from "../hooks/useTheme";

export function ThemeToggle() {
  const { isDark, toggle } = useTheme();

  return (
    <button
      onClick={toggle}
      className="p-2 rounded-lg transition-colors hover:bg-[var(--bg-tertiary)]"
      title={isDark ? "Switch to light theme" : "Switch to dark theme"}
    >
      {isDark ? (
        <Sun className="w-5 h-5 text-[var(--text-secondary)]" />
      ) : (
        <Moon className="w-5 h-5 text-[var(--text-secondary)]" />
      )}
    </button>
  );
}
