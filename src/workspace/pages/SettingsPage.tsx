import { getVersion } from "@tauri-apps/api/app";
import { isTauri } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Button, Card, PageHeader, Row, Segmented, Switch } from "../../components/ui";
import { type LocalePref, useLocaleStore, useT } from "../../i18n";
import { useDockStore } from "../../store/dock";
import { useLibraryStore } from "../../store/library";
import { type ThemePref, useThemeStore } from "../../store/theme";

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
  const { status, setEnabled, setPinned } = useDockStore();
  const { overview, busy, load, chooseDir } = useLibraryStore();
  const resetDir = useLibraryStore((s) => s.useDefaultDir);
  const [version, setVersion] = useState<string | null>(null);

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
          <Switch
            label={t("settings.dock")}
            checked={status?.enabled ?? false}
            disabled={!status}
            onChange={(on) => void setEnabled(on)}
          />
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

      <Card title={t("settings.library")}>
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
        <Row title={t("settings.version")}>
          <span className="text-sm text-text-muted">{version ?? "-"}</span>
        </Row>
        <Row title={t("settings.license")} description={t("settings.licenseHint")} />
      </Card>
    </div>
  );
}
