import { invoke, isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, ExternalLink, type LucideIcon, Plug, Puzzle, RefreshCw, Store } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button, IconButton } from "../../components/ui";
import { type MessageKey, useT } from "../../i18n";

// Mirrors src-tauri/src/extensions.rs.
type Kind = "plugin" | "marketplace" | "mcp";
interface Extension {
  id: string;
  name: string;
  kind: Kind;
  repo: string;
  url: string;
  commit: string;
  license: string;
  target: string;
  installed: boolean;
}
interface Listing {
  claudeFound: boolean;
  extensions: Extension[];
}

const KIND: Record<Kind, { label: MessageKey; Icon: LucideIcon; color: string }> = {
  plugin: { label: "ext.kind.plugin", Icon: Puzzle, color: "var(--c-kind-skill)" },
  marketplace: { label: "ext.kind.marketplace", Icon: Store, color: "var(--c-kind-agent)" },
  mcp: { label: "ext.kind.mcp", Icon: Plug, color: "var(--c-kind-rule)" },
};

/** Descriptions and notes by extension id (the list itself comes from the backend). */
const TEXT: Record<string, { description: MessageKey; note?: MessageKey }> = {
  rea: { description: "ext.desc.rea", note: "ext.note.rea" },
  superpowers: { description: "ext.desc.superpowers", note: "ext.note.superpowers" },
  "compound-engineering": { description: "ext.desc.compoundEngineering" },
  "agent-skills": { description: "ext.desc.agentSkills" },
  "claude-code-workflows": { description: "ext.desc.wshobson" },
  "voltagent-subagents": { description: "ext.desc.voltagent" },
};

const open = (url: string) => {
  if (isTauri()) void openUrl(url);
  else window.open(url, "_blank");
};

/** Cards for the open-source Claude Code extensions the app can install. */
export function ExtensionGallery() {
  const t = useT();
  const [listing, setListing] = useState<Listing | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);

  const reload = useCallback(async () => {
    if (!isTauri()) return;
    setListing(null);
    try {
      setListing(await invoke<Listing>("extensions_list"));
    } catch (e) {
      setMessage({ text: String(e), error: true });
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const run = async (ext: Extension) => {
    setBusy(ext.id);
    setMessage(null);
    try {
      await invoke(ext.installed ? "extension_uninstall" : "extension_install", { id: ext.id });
      setMessage({
        text: ext.installed
          ? t("ext.removed")
          : ext.kind === "marketplace"
            ? `${t("ext.done")} ${t("ext.marketplaceHint")}`
            : t("ext.done"),
        error: false,
      });
    } catch (e) {
      setMessage({ text: String(e), error: true });
    }
    setBusy(null);
    await reload();
  };

  return (
    <section className="mb-10">
      <div className="mb-1 flex items-center gap-2">
        <h2 className="text-sm font-semibold">{t("ext.title")}</h2>
        <IconButton
          label={t("ext.refresh")}
          className="size-6"
          onClick={() => void reload()}
          disabled={busy !== null}
        >
          <RefreshCw size={13} />
        </IconButton>
      </div>
      <p className="mb-3 text-xs text-text-muted">{t("ext.description")}</p>
      {listing && !listing.claudeFound && (
        <p className="mb-3 text-xs text-warning">{t("ext.noClaude")}</p>
      )}
      {message && (
        <p
          className={`selectable mb-3 text-xs whitespace-pre-wrap ${message.error ? "text-danger" : "text-success"}`}
        >
          {message.text}
        </p>
      )}
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {(listing?.extensions ?? []).map((ext) => {
          const kind = KIND[ext.kind];
          const text = TEXT[ext.id];
          return (
            <article
              key={ext.id}
              className="flex flex-col rounded-xl border border-border bg-surface-raised p-3.5 shadow-card"
            >
              <div className="mb-2 flex items-start gap-2.5">
                <span
                  className="flex size-8 shrink-0 items-center justify-center rounded-lg"
                  style={{
                    color: kind.color,
                    background: `color-mix(in srgb, ${kind.color} 14%, transparent)`,
                  }}
                >
                  <kind.Icon size={16} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="truncate text-sm font-semibold">{ext.name}</span>
                    {ext.installed && (
                      <span className="flex shrink-0 items-center gap-0.5 text-fs-10.5 text-success">
                        <Check size={11} />
                        {t("ext.installed")}
                      </span>
                    )}
                  </div>
                  <div className="flex flex-wrap items-center gap-1 text-fs-10.5 text-text-subtle">
                    <span
                      className="rounded px-1 font-medium"
                      style={{
                        color: kind.color,
                        background: `color-mix(in srgb, ${kind.color} 10%, transparent)`,
                      }}
                    >
                      {t(kind.label)}
                    </span>
                    <span className="rounded bg-sunken px-1">{ext.license}</span>
                    <button
                      type="button"
                      onClick={() => open(ext.url)}
                      title={t("ext.checkedAt", { commit: ext.commit })}
                      className="flex min-w-0 items-center gap-0.5 hover:text-text"
                    >
                      <span className="truncate">{ext.repo}</span>
                      <ExternalLink size={10} className="shrink-0" />
                    </button>
                  </div>
                </div>
              </div>
              {text && <p className="mb-1 flex-1 text-xs text-text-muted">{t(text.description)}</p>}
              {text?.note && <p className="mb-1 text-fs-11 text-text-subtle">{t(text.note)}</p>}
              <div className="mt-2 flex items-center justify-between gap-2">
                <code className="truncate text-fs-10.5 text-text-subtle" title={ext.target}>
                  {ext.target}
                </code>
                <Button
                  size="sm"
                  variant={ext.installed ? "secondary" : "primary"}
                  disabled={!listing?.claudeFound || busy !== null}
                  onClick={() => void run(ext)}
                >
                  {busy === ext.id
                    ? t("ext.working")
                    : t(ext.installed ? "ext.uninstall" : "ext.install")}
                </Button>
              </div>
            </article>
          );
        })}
        {!listing && isTauri() && <p className="text-xs text-text-subtle">{t("ext.checking")}</p>}
      </div>
    </section>
  );
}
