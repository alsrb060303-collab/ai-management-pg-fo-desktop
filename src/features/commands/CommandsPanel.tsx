import { invoke, isTauri } from "@tauri-apps/api/core";
import { Check, Copy, RefreshCw, Search } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { type MessageKey, useT } from "../../i18n";
import { useProjectStore } from "../../store/project";

// Mirrors `SlashCommand` in src-tauri/src/claude_config/mod.rs.
interface SlashCommand {
  name: string;
  kind: "skill" | "command";
  scope: "managed" | "user" | "parent" | "project" | "local";
  description: string | null;
  argumentHint: string | null;
  manual: boolean;
  path: string;
}

const SCOPE_ORDER: SlashCommand["scope"][] = ["project", "local", "parent", "user", "managed"];

/** The project's / commands, grouped by where they come from; click copies. */
export function CommandsPanel({ compact = false }: { compact?: boolean }) {
  const t = useT();
  const project = useProjectStore((s) => s.current);
  const [list, setList] = useState<SlashCommand[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [copied, setCopied] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!isTauri() || !project) return;
    try {
      setList(await invoke<SlashCommand[]>("claude_commands", { path: project }));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [project]);

  useEffect(() => {
    setList(null);
    void load();
  }, [load]);

  // Skills added while the app runs show up when the window is used again.
  useEffect(() => {
    const onFocus = () => void load();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [load]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (list ?? []).filter(
      (c) =>
        !q || c.name.toLowerCase().includes(q) || (c.description ?? "").toLowerCase().includes(q),
    );
  }, [list, query]);

  const copy = (c: SlashCommand) => {
    const text = `/${c.name} `;
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(c.name);
      setTimeout(() => setCopied((n) => (n === c.name ? null : n)), 1200);
    });
  };

  if (!project) return <p className="text-xs text-text-muted">{t("commands.noProject")}</p>;

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-1.5">
        <label className="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-lg border border-border bg-surface-raised px-2.5 focus-within:border-border-strong">
          <Search size={13} className="shrink-0 text-text-subtle" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("commands.search")}
            aria-label={t("commands.search")}
            className="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-text-subtle"
          />
        </label>
        <button
          type="button"
          onClick={() => void load()}
          title={t("commands.refresh")}
          aria-label={t("commands.refresh")}
          className="shrink-0 rounded p-1.5 text-text-muted hover:bg-hover hover:text-text"
        >
          <RefreshCw size={13} />
        </button>
      </div>
      {error && <p className="selectable text-xs text-danger">{error}</p>}
      {list && list.length === 0 && (
        <p className="text-xs text-text-muted">{t("commands.empty")}</p>
      )}
      {list && list.length > 0 && shown.length === 0 && (
        <p className="text-xs text-text-muted">{t("commands.noMatch")}</p>
      )}
      {SCOPE_ORDER.map((scope) => {
        const items = shown.filter((c) => c.scope === scope);
        if (!items.length) return null;
        return (
          <section key={scope}>
            <h3 className="mb-1 text-[11px] font-semibold text-text-muted">
              {t(`commands.scope.${scope}` as MessageKey)}{" "}
              <span className="font-normal">{items.length}</span>
            </h3>
            <ul className="space-y-1">
              {items.map((c) => (
                <li key={`${c.scope}:${c.kind}:${c.name}`}>
                  <button
                    type="button"
                    onClick={() => copy(c)}
                    title={`${t("commands.copy")} · ${c.path}`}
                    className="group flex w-full items-start gap-2 rounded-lg border border-border bg-surface-raised px-2.5 py-2 text-left hover:border-border-strong"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-baseline gap-x-1.5">
                        <span className="font-mono text-xs font-medium">/{c.name}</span>
                        {c.argumentHint && (
                          <span className="font-mono text-[11px] text-text-subtle">
                            {c.argumentHint}
                          </span>
                        )}
                        {c.manual && (
                          <span
                            title={t("commands.manualHint")}
                            className="rounded bg-sunken px-1 text-[10px] text-text-subtle"
                          >
                            {c.kind === "command" ? t("commands.command") : t("commands.manual")}
                          </span>
                        )}
                      </div>
                      {c.description && (
                        <p
                          className={`mt-0.5 text-[11px] leading-snug text-text-muted ${compact ? "line-clamp-2" : ""}`}
                        >
                          {c.description}
                        </p>
                      )}
                    </div>
                    <span className="mt-0.5 shrink-0 text-text-subtle group-hover:text-text">
                      {copied === c.name ? (
                        <Check size={13} className="text-success" />
                      ) : (
                        <Copy size={13} />
                      )}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        );
      })}
      {list && <p className="text-[11px] text-text-subtle">{t("commands.plugins")}</p>}
    </div>
  );
}
