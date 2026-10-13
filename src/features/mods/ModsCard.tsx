import { invoke, isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button, Card, Row } from "../../components/ui";
import { type MessageKey, useT } from "../../i18n";

// Mirrors src-tauri/src/claude_mod.rs.
interface ModStatus {
  claudeVersion: string | null;
  supported: boolean;
  installed: string[];
}

const OWN = "branch-notes@ai-management";

/**
 * Mods the app can install, checked against their repositories (licenses in
 * docs/research.md). The ids must match `RECOMMENDED` in claude_mod.rs.
 */
const RECOMMENDED: {
  id: string;
  name: string;
  description: MessageKey;
  url: string;
  license: string;
  note?: MessageKey;
}[] = [
  {
    id: "token-weather@claude-code-playground-mods",
    name: "token-weather",
    description: "mods.rec.tokenWeather",
    url: "https://github.com/anthropics/claude-code-playground/tree/main/claude-code/mods/token-weather",
    license: "Apache-2.0",
  },
  {
    id: "blast-radius@claude-code-playground-mods",
    name: "blast-radius",
    description: "mods.rec.blastRadius",
    url: "https://github.com/anthropics/claude-code-playground/tree/main/claude-code/mods/blast-radius",
    license: "Apache-2.0",
  },
  {
    id: "replay-theater@claude-code-playground-mods",
    name: "replay-theater",
    description: "mods.rec.replayTheater",
    url: "https://github.com/anthropics/claude-code-playground/tree/main/claude-code/mods/replay-theater",
    license: "Apache-2.0",
  },
  {
    id: "claude-hud@claude-hud",
    name: "claude-hud",
    description: "mods.rec.claudeHud",
    url: "https://github.com/jarrodwatts/claude-hud",
    license: "MIT",
    note: "mods.rec.claudeHudNote",
  },
];

const open = (url: string) => {
  if (isTauri()) void openUrl(url);
  else window.open(url, "_blank");
};

/** Settings card: the app's own Claude Code mod and recommended ones. */
export function ModsCard() {
  const t = useT();
  const [status, setStatus] = useState<ModStatus | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);

  const reload = useCallback(async () => {
    if (!isTauri()) return;
    setStatus(await invoke<ModStatus>("claude_mod_status"));
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const run = async (id: string, install: boolean) => {
    setBusy(id);
    setMessage(null);
    try {
      if (!install) await invoke("claude_mod_uninstall", { id });
      else if (id === OWN) await invoke("claude_mod_install");
      else await invoke("claude_mod_install_recommended", { id });
      setMessage({ text: t(install ? "mods.installed" : "mods.uninstalled"), error: false });
    } catch (e) {
      setMessage({ text: String(e), error: true });
    }
    setBusy(null);
    await reload();
  };

  const ready = !!status?.claudeVersion && status.supported;
  const toggle = (id: string) => {
    const installed = status?.installed.includes(id) ?? false;
    return (
      <Button
        size="sm"
        variant={installed ? "secondary" : "primary"}
        disabled={!ready || busy !== null}
        onClick={() => void run(id, !installed)}
      >
        {busy === id ? t("mods.working") : t(installed ? "mods.uninstall" : "mods.install")}
      </Button>
    );
  };

  return (
    <Card title={t("mods.title")}>
      <Row
        title={t("mods.claude")}
        description={
          !status
            ? t("mods.checking")
            : !status.claudeVersion
              ? t("mods.noClaude")
              : !status.supported
                ? t("mods.tooOld", { version: status.claudeVersion })
                : status.claudeVersion
        }
      >
        <Button size="sm" variant="ghost" onClick={() => void reload()}>
          {t("mods.recheck")}
        </Button>
      </Row>
      <Row title={t("mods.own.title")} description={t("mods.own.description")}>
        {toggle(OWN)}
      </Row>
      {message && (
        <p
          className={`selectable px-4 py-2 text-xs whitespace-pre-wrap ${message.error ? "text-danger" : "text-text-muted"}`}
        >
          {message.text}
        </p>
      )}
      <p className="px-4 pt-3 text-xs font-semibold text-text-muted">{t("mods.recommended")}</p>
      {RECOMMENDED.map((m) => (
        <Row
          key={m.id}
          title={
            <span className="flex items-center gap-1.5">
              {m.name}
              <button
                type="button"
                onClick={() => open(m.url)}
                title={m.url}
                className="flex items-center gap-0.5 text-fs-11 font-normal text-text-subtle hover:text-text"
              >
                {m.license}
                <ExternalLink size={10} />
              </button>
            </span>
          }
          description={
            <>
              {t(m.description)}
              {m.note && <span className="block text-text-subtle">{t(m.note)}</span>}
            </>
          }
        >
          {toggle(m.id)}
        </Row>
      ))}
    </Card>
  );
}
