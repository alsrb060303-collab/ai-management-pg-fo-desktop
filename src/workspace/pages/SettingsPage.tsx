import { getVersion } from "@tauri-apps/api/app";
import { isTauri } from "@tauri-apps/api/core";
import { Magnet } from "lucide-react";
import { useEffect, useState } from "react";
import { Button, Card, PageHeader, Row, Segmented, Switch } from "../../components/ui";
import { ModsCard } from "../../features/mods/ModsCard";
import { type LocalePref, timeAgo, useLocaleStore, useT } from "../../i18n";
import { useAccountStore } from "../../store/account";
import { useDockStore } from "../../store/dock";
import { FONT_SIZES, useFontSizeStore } from "../../store/fontSize";
import { useLibraryStore } from "../../store/library";
import { useNotifyStore } from "../../store/notify";
import { useProjectStore } from "../../store/project";
import { useSyncStore } from "../../store/sync";
import { type ThemePref, useThemeStore } from "../../store/theme";
import { useUpdaterStore } from "../../store/updater";
import { TOOL_IDS, TOOLS } from "../../tools";

const THEMES: ThemePref[] = ["system", "light", "dark"];

export function SettingsPage() {
  const t = useT();
  const { pref, setPref } = useThemeStore();
  const language = useLocaleStore((s) => s.pref);
  const setLanguage = useLocaleStore((s) => s.setPref);
  const languages: { value: LocalePref; label: string }[] = [
    { value: "system", label: t("settings.language.system") },
    { value: "ko", label: "한국어" },
    { value: "en", label: "English" },
  ];
  const { status, setPinned } = useDockStore();
  const fontSize = useFontSizeStore((s) => s.size);
  const setFontSize = useFontSizeStore((s) => s.setSize);
  const { overview, busy, load, chooseDir } = useLibraryStore();
  const resetDir = useLibraryStore((s) => s.useDefaultDir);
  const defaultTool = useProjectStore((s) => s.defaultTool);
  const setDefaultTool = useProjectStore((s) => s.setDefaultTool);
  const [version, setVersion] = useState<string | null>(null);
  const update = useUpdaterStore();
  const updateLine =
    update.status === "checking"
      ? t("update.checking")
      : update.status === "none"
        ? t("update.none")
        : update.status === "available"
          ? t("update.available", { version: update.version ?? "" })
          : update.status === "downloading"
            ? t("update.downloading")
            : update.status === "error"
              ? t("update.failed", { error: update.error ?? "" })
              : undefined;

  useEffect(() => {
    void load();
    if (isTauri()) getVersion().then(setVersion, () => {});
  }, [load]);

  return (
    <div className="mx-auto max-w-2xl space-y-6 px-8 py-10">
      <PageHeader title={t("settings.title")} />

      <Card title={t("settings.display")}>
        <Row title={t("settings.theme")} description={t("settings.themeHint")}>
          <Segmented
            label={t("settings.theme")}
            value={pref}
            options={THEMES.map((value) => ({ value, label: t(`settings.theme.${value}`) }))}
            onChange={setPref}
          />
        </Row>
        <Row title={t("settings.fontSize")} description={t("settings.fontSizeHint")}>
          <Segmented
            label={t("settings.fontSize")}
            value={fontSize}
            options={FONT_SIZES.map((value) => ({
              value,
              label: t(`settings.fontSize.${value}`),
            }))}
            onChange={setFontSize}
          />
        </Row>
        <Row title={t("settings.language")} description={t("settings.languageHint")}>
          <Segmented
            label={t("settings.language")}
            value={language}
            options={languages}
            onChange={setLanguage}
          />
        </Row>
      </Card>

      <Card title={t("settings.sidebar")}>
        <Row title={t("settings.dock")} description={t("settings.dockHint")}>
          <Magnet size={16} className="shrink-0 text-text-muted" />
        </Row>
        <Row title={t("settings.pin")} description={t("settings.pinHint")}>
          <Switch
            label={t("settings.pin")}
            checked={status?.pinned ?? false}
            disabled={!status}
            onChange={(on) => void setPinned(on)}
          />
        </Row>
        <Row title={t("settings.shortcut")}>
          <kbd className="rounded-md border border-border bg-sunken px-2 py-0.5 font-mono text-xs">
            Ctrl+Alt+Space
          </kbd>
        </Row>
      </Card>

      <NotifyCard />

      <SyncCard />

      <ModsCard />

      <Card title={t("settings.library")}>
        <Row title={t("tools.default")} description={t("tools.defaultHint")}>
          <Segmented
            label={t("tools.default")}
            value={defaultTool}
            options={TOOL_IDS.map((id) => ({ value: id, label: TOOLS[id].name }))}
            onChange={(tool) => void setDefaultTool(tool)}
          />
        </Row>
        <Row
          title={t("settings.libraryDir")}
          description={
            <span className="selectable break-all">
              {overview ? overview.dir : t("settings.loading")}
            </span>
          }
        >
          {overview && !overview.isDefault && (
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => void resetDir()}>
              {t("settings.default")}
            </Button>
          )}
          <Button size="sm" disabled={busy} onClick={() => void chooseDir()}>
            {t("settings.changeFolder")}
          </Button>
        </Row>
      </Card>

      <Card title={t("settings.about")}>
        <Row title={t("settings.version")} description={updateLine}>
          <span className="text-sm text-text-muted">{version ?? "-"}</span>
          <Button
            size="sm"
            disabled={update.status === "checking" || update.status === "downloading"}
            onClick={() =>
              update.status === "available" ? void update.install() : void update.check(true)
            }
          >
            {update.status === "available" ? t("update.install") : t("update.check")}
          </Button>
        </Row>
        <Row title={t("settings.license")} description={t("settings.licenseHint")} />
      </Card>
    </div>
  );
}

function NotifyCard() {
  const t = useT();
  const { on, setOn } = useNotifyStore();
  return (
    <Card title={t("settings.notify")}>
      <Row title={t("settings.notifyGitHub")} description={t("settings.notifyGitHubHint")}>
        <Switch label={t("settings.notifyGitHub")} checked={on} onChange={setOn} />
      </Row>
    </Card>
  );
}

function SyncCard() {
  const t = useT();
  const { status, init, syncNow, setEnabled } = useSyncStore();
  const account = useAccountStore((s) => s.info);
  const openAccount = useAccountStore((s) => s.openDialog);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    void init();
  }, [init]);

  const partial = useSyncStore((s) => s.partial);
  const state = status?.state ?? "notConfigured";
  const configured = state !== "notConfigured";
  const on = configured && state !== "off";
  const last = status?.lastSync
    ? t("settings.sync.idle", { time: timeAgo(Math.floor(status.lastSync / 1000)) })
    : t("settings.sync.never");
  const detail =
    state === "syncing"
      ? t("settings.sync.syncing")
      : state === "idle"
        ? last
        : state === "error"
          ? (status?.message ?? "")
          : t(`settings.sync.${state}`);

  return (
    <Card title={t("settings.sync")}>
      {configured && (
        <Row
          title={t("account.title")}
          description={
            account?.uid
              ? (account.profile?.email ?? account.profile?.name ?? account.uid)
              : t("account.intro")
          }
        >
          <Button size="sm" variant={account?.uid ? "secondary" : "primary"} onClick={openAccount}>
            {account?.uid ? t("settings.account.manage") : t("account.signIn")}
          </Button>
        </Row>
      )}
      <Row title={t("settings.sync.title")} description={t("settings.sync.hint")}>
        <Switch
          label={t("settings.sync.title")}
          checked={on}
          disabled={!configured}
          onChange={(next) => void setEnabled(next)}
        />
      </Row>
      {configured && (
        <Row
          title={<span className={state === "error" ? "text-danger" : ""}>{detail}</span>}
          description={
            state === "error"
              ? last
              : partial.length > 0
                ? t("settings.sync.partial", { names: partial.join(", ") })
                : undefined
          }
        >
          {on && state !== "needsSignIn" && state !== "notAllowed" && (
            <Button size="sm" disabled={state === "syncing"} onClick={syncNow}>
              {t("settings.sync.now")}
            </Button>
          )}
        </Row>
      )}
      {status?.uid && (state === "notAllowed" || state === "error" || state === "idle") && (
        <Row
          title={t("settings.sync.accountId")}
          description={<span className="selectable font-mono">{status.uid}</span>}
        >
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              void navigator.clipboard.writeText(status.uid ?? "").then(() => {
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              });
            }}
          >
            {copied ? t("settings.sync.copied") : t("settings.sync.copy")}
          </Button>
        </Row>
      )}
    </Card>
  );
}
