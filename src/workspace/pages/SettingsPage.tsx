import { getVersion } from "@tauri-apps/api/app";
import { isTauri } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Button, Card, PageHeader, Row, Segmented, Switch } from "../../components/ui";
import { useDockStore } from "../../store/dock";
import { useLibraryStore } from "../../store/library";
import { type ThemePref, useThemeStore } from "../../store/theme";

const THEMES: { value: ThemePref; label: string }[] = [
  { value: "system", label: "시스템" },
  { value: "light", label: "라이트" },
  { value: "dark", label: "다크" },
];

export function SettingsPage() {
  const { pref, setPref } = useThemeStore();
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
      <PageHeader title="설정" />

      <Card title="화면">
        <Row title="테마" description="시스템을 고르면 OS의 라이트 · 다크 설정을 따라가요.">
          <Segmented label="테마" value={pref} options={THEMES} onChange={setPref} />
        </Row>
      </Card>

      <Card title="사이드바">
        <Row
          title="터미널 옆에 붙이기"
          description="Windows Terminal · VS Code · Claude 데스크탑 중 지금 쓰는 창 옆에 자동으로 붙어요."
        >
          <Switch
            label="터미널 옆에 붙이기"
            checked={status?.enabled ?? false}
            disabled={!status}
            onChange={(on) => void setEnabled(on)}
          />
        </Row>
        <Row title="항상 위" description="다른 창보다 항상 위에 보여요.">
          <Switch
            label="항상 위"
            checked={status?.pinned ?? false}
            disabled={!status}
            onChange={(on) => void setPinned(on)}
          />
        </Row>
        <Row title="보이기 / 숨기기 단축키">
          <kbd className="rounded-md border border-border bg-sunken px-2 py-0.5 font-mono text-xs">
            Ctrl+Alt+Space
          </kbd>
        </Row>
      </Card>

      <Card title="라이브러리">
        <Row
          title="라이브러리 폴더"
          description={
            <span className="selectable break-all">{overview ? overview.dir : "불러오는 중…"}</span>
          }
        >
          {overview && !overview.isDefault && (
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => void resetDir()}>
              기본값
            </Button>
          )}
          <Button size="sm" disabled={busy} onClick={() => void chooseDir()}>
            폴더 변경
          </Button>
        </Row>
      </Card>

      <Card title="정보">
        <Row title="버전">
          <span className="text-sm text-text-muted">{version ?? "-"}</span>
        </Row>
        <Row
          title="라이선스"
          description="Apache-2.0 오픈소스. Anthropic 또는 Claude와 관련 없는 독립 프로젝트예요."
        />
      </Card>
    </div>
  );
}
