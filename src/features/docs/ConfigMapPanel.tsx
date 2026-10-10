import { isTauri } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import {
  type ConfigItem,
  type ConfigKind,
  type ConfigScope,
  type ConfigState,
  useProjectStore,
} from "../../store/project";

const SECTIONS: { kind: ConfigKind; title: string; hint: string }[] = [
  { kind: "instructions", title: "지침", hint: "CLAUDE.md · 항상 읽힘" },
  { kind: "rule", title: "규칙", hint: ".claude/rules" },
  { kind: "skill", title: "Skills", hint: "필요할 때 읽힘" },
  { kind: "agent", title: "Subagents", hint: "별도 컨텍스트에서 실행" },
  { kind: "command", title: "명령", hint: ".claude/commands" },
  { kind: "mcp", title: "MCP 서버", hint: ".mcp.json" },
  { kind: "plugin", title: "플러그인", hint: "enabledPlugins" },
  { kind: "settings", title: "설정 파일", hint: "settings.json" },
];

const SCOPE_LABELS: Record<ConfigScope, string> = {
  managed: "관리",
  user: "개인",
  parent: "상위 폴더",
  project: "프로젝트",
  local: "로컬",
};

const STATE_STYLES: Record<ConfigState, { label: string; className: string }> = {
  active: { label: "사용 중", className: "text-emerald-400" },
  conditional: { label: "조건부", className: "text-sky-400" },
  limited: { label: "제한", className: "text-amber-400" },
  off: { label: "꺼짐", className: "text-text-muted" },
  shadowed: { label: "가려짐", className: "text-text-muted" },
  notLoaded: { label: "안 읽힘", className: "text-text-muted" },
};

export function ConfigMapPanel() {
  const { current, recent, map, loading, error, choose, select, refresh } = useProjectStore();

  return (
    <div className="space-y-3">
      <ProjectBar
        current={current}
        recent={recent}
        loading={loading}
        onChoose={choose}
        onSelect={select}
        onRefresh={refresh}
      />
      {error && <p className="rounded-md bg-red-950/50 px-3 py-2 text-xs text-red-300">{error}</p>}
      {current && loading && !map && <p className="text-xs text-text-muted">읽는 중…</p>}
      {!current && (
        <p className="text-xs text-text-muted">
          프로젝트 폴더를 고르면 Claude Code가 그 프로젝트에서 읽는 설정을 범위별로 보여줍니다.
        </p>
      )}
      {map &&
        SECTIONS.map((section) => {
          const items = map.items.filter((i) => i.kind === section.kind);
          if (items.length === 0) return null;
          return (
            <section key={section.kind}>
              <h3 className="mb-1.5 flex items-baseline gap-2 text-xs font-semibold">
                {section.title}
                <span className="font-normal text-text-muted">
                  {section.hint} · {items.length}
                </span>
              </h3>
              <ul className="space-y-1.5">
                {items.map((item) => (
                  <ConfigRow key={`${item.scope}:${item.path}:${item.name}`} item={item} />
                ))}
              </ul>
            </section>
          );
        })}
      {map && map.items.length === 0 && (
        <p className="text-xs text-text-muted">
          이 프로젝트에 적용되는 Claude Code 설정이 없습니다.
        </p>
      )}
      {map && map.notes.length > 0 && (
        <ul className="space-y-0.5 border-t border-border pt-2 text-[11px] text-text-muted">
          {map.notes.map((note) => (
            <li key={note}>· {note}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

interface ProjectBarProps {
  current: string | null;
  recent: string[];
  loading: boolean;
  onChoose: () => void;
  onSelect: (path: string) => void;
  onRefresh: () => void;
}

function ProjectBar({ current, recent, loading, onChoose, onSelect, onRefresh }: ProjectBarProps) {
  return (
    <div className="flex items-center gap-1.5">
      <select
        aria-label="최근 프로젝트"
        value={current ?? ""}
        onChange={(e) => e.target.value && onSelect(e.target.value)}
        className="min-w-0 flex-1 truncate rounded-md border border-border bg-surface-raised px-2 py-1.5 text-xs"
        title={current ?? undefined}
      >
        {!current && <option value="">프로젝트 없음</option>}
        {(current && !recent.includes(current) ? [current, ...recent] : recent).map((path) => (
          <option key={path} value={path}>
            {baseName(path)}
          </option>
        ))}
      </select>
      <button type="button" onClick={onChoose} className={buttonClass}>
        폴더 선택
      </button>
      <button
        type="button"
        onClick={onRefresh}
        disabled={!current || loading}
        className={buttonClass}
        title="다시 읽기"
      >
        {loading ? "…" : "↻"}
      </button>
    </div>
  );
}

function ConfigRow({ item }: { item: ConfigItem }) {
  const state = STATE_STYLES[item.state];
  const dimmed = item.state === "shadowed" || item.state === "off" || item.state === "notLoaded";

  const [failed, setFailed] = useState(false);
  const reveal = () => {
    if (!isTauri() || !item.path) return;
    revealItemInDir(item.path).then(
      () => setFailed(false),
      () => setFailed(true),
    );
  };

  return (
    <li>
      <button
        type="button"
        onClick={reveal}
        disabled={!item.path}
        title={item.path || undefined}
        className="w-full rounded-md border border-border bg-surface-raised px-2.5 py-1.5 text-left hover:border-text-muted disabled:cursor-default"
      >
        <div className="flex items-center gap-1.5 text-xs">
          <span
            className={`min-w-0 flex-1 truncate font-medium ${dimmed ? "text-text-muted line-through decoration-text-muted/60" : ""}`}
          >
            {item.name}
          </span>
          <span className="shrink-0 rounded bg-border px-1 text-[10px] text-text-muted">
            {SCOPE_LABELS[item.scope]}
          </span>
          <span className={`shrink-0 text-[10px] ${state.className}`}>{state.label}</span>
        </div>
        {item.description && (
          <p className="mt-0.5 line-clamp-2 text-[11px] text-text-muted">{item.description}</p>
        )}
        {item.detail && <p className="mt-0.5 text-[10px] text-text-muted/80">{item.detail}</p>}
        {failed && (
          <p className="mt-0.5 text-[10px] text-red-300">
            파일을 열 수 없습니다. 다시 읽기(↻)를 눌러 주세요.
          </p>
        )}
      </button>
    </li>
  );
}

const buttonClass =
  "shrink-0 rounded-md border border-border bg-surface-raised px-2 py-1.5 text-xs hover:border-text-muted disabled:opacity-50";

function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
