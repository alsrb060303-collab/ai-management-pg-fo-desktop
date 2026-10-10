import { useEffect } from "react";
import { type ApplyStatus, type LibraryItem, useLibraryStore } from "../../store/library";
import { useProjectStore } from "../../store/project";
import { ModeButtons, smallButton } from "./ModeButtons";

const STATUS_TEXT: Record<ApplyStatus, { label: string; className: string }> = {
  current: { label: "적용됨", className: "text-emerald-400" },
  outdated: { label: "업데이트 있음", className: "text-sky-400" },
  modified: { label: "프로젝트에서 수정됨", className: "text-amber-400" },
  missing: { label: "복사본 없음", className: "text-amber-400" },
  conflict: { label: "같은 이름이 이미 있음", className: "text-text-muted" },
};

export function LibraryPanel() {
  const current = useProjectStore((s) => s.current);
  const { overview, busy, error, load, chooseDir, useDefaultDir } = useLibraryStore();

  // Reload whenever the selected project changes (statuses are per project).
  useEffect(() => {
    void current;
    void load();
  }, [current, load]);

  const skills = overview?.items.filter((i) => i.kind === "skill") ?? [];
  const agents = overview?.items.filter((i) => i.kind === "agent") ?? [];

  return (
    <div className="space-y-3">
      <div className="rounded-md border border-border bg-surface-raised px-2.5 py-2 text-xs">
        <div className="flex items-center gap-1.5">
          <span className="min-w-0 flex-1 truncate text-text-muted" title={overview?.dir}>
            {overview ? overview.dir : "…"}
          </span>
          <button type="button" onClick={chooseDir} className={smallButton} disabled={busy}>
            폴더 변경
          </button>
          {overview && !overview.isDefault && (
            <button type="button" onClick={useDefaultDir} className={smallButton} disabled={busy}>
              기본값
            </button>
          )}
        </div>
        <p className="mt-1 text-[10px] text-text-muted">
          skills/&lt;이름&gt;/SKILL.md, agents/&lt;이름&gt;.md 구조. git 레포 폴더를 고르면 여러
          PC에서 같이 쓸 수 있습니다.
        </p>
      </div>

      {error && <p className="rounded-md bg-red-950/50 px-3 py-2 text-xs text-red-300">{error}</p>}
      {!current && (
        <p className="text-xs text-text-muted">
          설정 지도에서 프로젝트를 고르면 항목을 그 프로젝트에 적용하거나 뺄 수 있습니다.
        </p>
      )}

      <Section title="Skills" items={skills} />
      <Section title="Subagents" items={agents} />
      {overview && overview.items.length === 0 && (
        <p className="text-xs text-text-muted">
          라이브러리가 비어 있습니다. 설정 지도에서 skill·agent의 "라이브러리에 추가"를 누르거나
          폴더에 직접 넣어 주세요.
        </p>
      )}
    </div>
  );
}

function Section({ title, items }: { title: string; items: LibraryItem[] }) {
  if (items.length === 0) return null;
  return (
    <section>
      <h3 className="mb-1.5 text-xs font-semibold">
        {title} <span className="font-normal text-text-muted">· {items.length}</span>
      </h3>
      <ul className="space-y-1.5">
        {items.map((item) => (
          <LibraryRow key={`${item.kind}:${item.id}`} item={item} />
        ))}
      </ul>
    </section>
  );
}

function LibraryRow({ item }: { item: LibraryItem }) {
  const current = useProjectStore((s) => s.current);
  const { busy, apply, remove, update, forget } = useLibraryStore();
  const status = item.status ? STATUS_TEXT[item.status] : null;
  const applied = item.status !== null && item.status !== "conflict";

  return (
    <li className="rounded-md border border-border bg-surface-raised px-2.5 py-1.5">
      <div className="flex items-center gap-1.5 text-xs">
        <span className="min-w-0 flex-1 truncate font-medium" title={item.path}>
          {item.name}
        </span>
        {item.mode && (
          <span className="shrink-0 rounded bg-border px-1 text-[10px] text-text-muted">
            {item.mode === "personal" ? "나만" : "팀"}
          </span>
        )}
        {status && (
          <span className={`shrink-0 text-[10px] ${status.className}`}>{status.label}</span>
        )}
      </div>
      {item.description && (
        <p className="mt-0.5 line-clamp-2 text-[11px] text-text-muted">{item.description}</p>
      )}
      {current && (
        <div className="mt-1.5 flex flex-wrap gap-1">
          {item.status === null && (
            <ModeButtons label="적용" disabled={busy} onPick={(mode) => apply(item, mode)} />
          )}
          {item.status === "outdated" && (
            <button
              type="button"
              className={smallButton}
              disabled={busy}
              onClick={() => update(item)}
            >
              업데이트
            </button>
          )}
          {item.status === "modified" && (
            <button
              type="button"
              className={smallButton}
              disabled={busy}
              onClick={() => update(item)}
            >
              라이브러리 버전으로 덮어쓰기
            </button>
          )}
          {applied && (
            <button
              type="button"
              className={smallButton}
              disabled={busy}
              onClick={() => (item.status === "missing" ? forget(item) : remove(item))}
            >
              {item.status === "missing" ? "기록 지우기" : "빼기"}
            </button>
          )}
        </div>
      )}
    </li>
  );
}
