import { invoke } from "@tauri-apps/api/core";
import { diffLines } from "diff";
import { ChevronDown, ChevronRight, FolderOpen, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button, IconButton } from "../../components/ui";
import type { ComposerDoc } from "../../store/composer";
import { useProjectStore } from "../../store/project";

// Mirrors `PlanItem` / `Status` in src-tauri/src/composer/write.rs.
type Status =
  | { kind: "new" | "update" | "unchanged" | "modified" | "exists" }
  | { kind: "blocked"; reason: string };

interface PlanItem {
  path: string;
  status: Status;
  personal: boolean;
  old: string | null;
  new: string;
}

interface WriteResult {
  written: string[];
  skipped: [string, string][];
}

const STATUS: Record<Status["kind"], { label: string; className: string }> = {
  new: { label: "새 파일", className: "text-success" },
  update: { label: "업데이트", className: "text-info" },
  unchanged: { label: "변경 없음", className: "text-text-subtle" },
  modified: { label: "직접 수정됨", className: "text-warning" },
  exists: { label: "이미 있는 파일", className: "text-warning" },
  blocked: { label: "쓸 수 없음", className: "text-danger" },
};

const needsConfirm = (s: Status) => s.kind === "modified" || s.kind === "exists";

export function WriteDialog({ doc, onClose }: { doc: ComposerDoc; onClose: () => void }) {
  const { current, choose, refresh } = useProjectStore();
  const [plan, setPlan] = useState<PlanItem[] | null>(null);
  const [confirmed, setConfirmed] = useState<Set<string>>(new Set());
  const [open, setOpen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<WriteResult | null>(null);

  const load = useCallback(async () => {
    if (!current) return;
    setPlan(null);
    setError(null);
    try {
      setPlan(await invoke<PlanItem[]>("composer_plan", { project: current, doc }));
    } catch (e) {
      setError(String(e));
    }
  }, [current, doc]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const writable =
    plan?.filter(
      (p) =>
        p.status.kind === "new" ||
        p.status.kind === "update" ||
        (needsConfirm(p.status) && confirmed.has(p.path)),
    ) ?? [];

  const write = async () => {
    if (!current) return;
    setBusy(true);
    setError(null);
    try {
      const res = await invoke<WriteResult>("composer_write", {
        project: current,
        doc,
        confirmed: [...confirmed],
      });
      setResult(res);
      void refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/30 p-6"
      onPointerDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="프로젝트에 쓰기"
        className="flex max-h-full w-full max-w-2xl flex-col rounded-2xl border border-border bg-surface-raised shadow-card"
      >
        <header className="flex items-center gap-2 border-b border-border px-5 py-3.5">
          <h2 className="font-serif text-lg font-medium">프로젝트에 쓰기</h2>
          <IconButton label="닫기" onClick={onClose} className="ml-auto">
            <X size={17} />
          </IconButton>
        </header>

        <div className="flex items-center gap-2 border-b border-border px-5 py-3 text-sm">
          <FolderOpen size={15} className="shrink-0 text-text-subtle" />
          <span className="selectable min-w-0 flex-1 truncate" title={current ?? undefined}>
            {current ?? "프로젝트를 골라 주세요"}
          </span>
          <Button size="sm" onClick={() => void choose()}>
            {current ? "바꾸기" : "폴더 선택"}
          </Button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-3">
          {error && (
            <p className="mb-3 rounded-lg bg-danger-soft px-3 py-2 text-xs text-danger">{error}</p>
          )}
          {result ? (
            <Result result={result} />
          ) : !current ? null : !plan ? (
            <p className="text-sm text-text-muted">확인하는 중…</p>
          ) : (
            <ul className="space-y-1.5">
              {plan.map((item) => {
                const status = STATUS[item.status.kind];
                const expanded = open === item.path;
                return (
                  <li key={item.path} className="rounded-lg border border-border">
                    <div className="flex items-center gap-2 px-3 py-2">
                      <button
                        type="button"
                        aria-label={expanded ? "변경 내용 접기" : "변경 내용 보기"}
                        onClick={() => setOpen(expanded ? null : item.path)}
                        className="text-text-subtle hover:text-text"
                      >
                        {expanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
                      </button>
                      <span className="min-w-0 flex-1 truncate font-mono text-[12px]">
                        {item.path}
                      </span>
                      {item.personal && (
                        <span className="rounded bg-sunken px-1 text-[10px] text-text-subtle">
                          나만
                        </span>
                      )}
                      <span className={`shrink-0 text-xs font-medium ${status.className}`}>
                        {status.label}
                      </span>
                    </div>
                    {item.status.kind === "blocked" && (
                      <p className="px-9 pb-2 text-xs text-danger">{item.status.reason}</p>
                    )}
                    {needsConfirm(item.status) && (
                      <label className="flex items-center gap-2 px-9 pb-2 text-xs text-text-muted">
                        <input
                          type="checkbox"
                          checked={confirmed.has(item.path)}
                          onChange={(e) => {
                            const next = new Set(confirmed);
                            if (e.target.checked) next.add(item.path);
                            else next.delete(item.path);
                            setConfirmed(next);
                          }}
                        />
                        {item.status.kind === "modified"
                          ? "직접 고친 내용을 버리고 덮어쓰기"
                          : "앱이 만들지 않은 파일이에요. 덮어쓰기"}
                      </label>
                    )}
                    {expanded && <Diff old={item.old ?? ""} next={item.new} />}
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        <footer className="flex items-center gap-2 border-t border-border px-5 py-3">
          {result ? (
            <Button variant="primary" className="ml-auto" onClick={onClose}>
              닫기
            </Button>
          ) : (
            <>
              <span className="text-xs text-text-subtle">
                쓰기 전에 변경 내용을 확인할 수 있어요.
              </span>
              <Button variant="ghost" className="ml-auto" onClick={onClose}>
                취소
              </Button>
              <Button
                variant="primary"
                disabled={!current || busy || writable.length === 0}
                onClick={() => void write()}
              >
                {busy ? "쓰는 중…" : `${writable.length}개 파일 쓰기`}
              </Button>
            </>
          )}
        </footer>
      </div>
    </div>
  );
}

function Result({ result }: { result: WriteResult }) {
  return (
    <div className="space-y-3 text-sm">
      <p>
        {result.written.length > 0
          ? `${result.written.length}개 파일을 썼어요.`
          : "쓴 파일이 없어요."}
      </p>
      {result.written.length > 0 && (
        <ul className="space-y-0.5 font-mono text-[12px] text-success">
          {result.written.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      )}
      {result.skipped.length > 0 && (
        <ul className="space-y-0.5 text-xs text-warning">
          {result.skipped.map(([p, why]) => (
            <li key={p}>
              <span className="font-mono">{p}</span>: {why}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Diff({ old, next }: { old: string; next: string }) {
  const parts = diffLines(old, next);
  return (
    <pre className="selectable max-h-72 overflow-auto border-t border-border bg-sunken px-3 py-2 font-mono text-[11.5px] leading-[1.55]">
      {parts.map((part, i) => {
        const cls = part.added
          ? "bg-success/15 text-success"
          : part.removed
            ? "bg-danger-soft text-danger line-through decoration-danger/40"
            : "text-text-muted";
        const sign = part.added ? "+ " : part.removed ? "- " : "  ";
        const lines = part.value.replace(/\n$/, "").split("\n");
        return (
          // biome-ignore lint/suspicious/noArrayIndexKey: diff hunks have no identity besides position
          <span key={i} className={`block ${cls}`}>
            {lines.map((l) => `${sign}${l}`).join("\n")}
          </span>
        );
      })}
    </pre>
  );
}
