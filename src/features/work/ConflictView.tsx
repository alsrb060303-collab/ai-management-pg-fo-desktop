import { invoke, isTauri } from "@tauri-apps/api/core";
import { Check, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { useT } from "../../i18n";
import { useWorkStore } from "../../store/work";

// Mirrors src-tauri/src/work/conflict.rs.
type Segment =
  | { kind: "text"; text: string }
  | {
      kind: "conflict";
      oursLabel: string;
      ours: string;
      base: string | null;
      theirsLabel: string;
      theirs: string;
    };

interface ConflictFile {
  segments: Segment[];
  markers: boolean;
  missing: boolean;
  unreadable: boolean;
}

type Choice = "ours" | "theirs" | "both" | "bothTheirsFirst";

const pick = (s: Extract<Segment, { kind: "conflict" }>, c: Choice) =>
  c === "ours"
    ? s.ours
    : c === "theirs"
      ? s.theirs
      : c === "both"
        ? s.ours + s.theirs
        : s.theirs + s.ours;

/**
 * A conflicted file, one block at a time: pick the current side, the
 * incoming side or both, then save it as resolved.
 */
export function ConflictView({ path, compact }: { path: string; compact: boolean }) {
  const t = useT();
  const { project, run, busy, overview } = useWorkStore();
  const [file, setFile] = useState<ConflictFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [choices, setChoices] = useState<Record<number, Choice>>({});

  useEffect(() => {
    setFile(null);
    setChoices({});
    if (!project || !isTauri()) return;
    invoke<ConflictFile>("work_conflict", { project, path }).then(setFile, (e) =>
      setError(String(e)),
    );
  }, [project, path]);

  if (error) return <p className="px-2 py-2 text-fs-11 text-danger">{error}</p>;
  if (!file)
    return <p className="px-2 py-2 text-fs-11 text-text-subtle">{t("work.diff.loading")}</p>;

  const box = "my-1 space-y-2 rounded-lg border border-danger/40 bg-surface-raised p-2.5 text-xs";
  const deleteButton = (
    <button
      type="button"
      disabled={busy}
      onClick={() => void run("work_resolve", { path, text: null })}
      className="rounded-md border border-border px-2.5 py-1 hover:border-border-strong disabled:opacity-50"
    >
      {t("work.conflict.delete")}
    </button>
  );

  if (file.missing)
    return (
      <div className={box}>
        <p className="text-text-muted">{t("work.conflict.missing")}</p>
        {deleteButton}
      </div>
    );
  if (file.unreadable)
    return (
      <div className={box}>
        <p className="text-text-muted">{t("work.conflict.unreadable")}</p>
      </div>
    );
  const whole = file.segments.map((s) => (s.kind === "text" ? s.text : "")).join("");
  if (!file.markers)
    return (
      <div className={box}>
        <p className="text-text-muted">{t("work.conflict.noMarkers")}</p>
        <div className="flex gap-2">
          <button
            type="button"
            disabled={busy}
            onClick={() => void run("work_resolve", { path, text: whole })}
            className="rounded-md bg-primary px-2.5 py-1 font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
          >
            {t("work.conflict.keepFile")}
          </button>
          {deleteButton}
        </div>
      </div>
    );

  const blocks = file.segments.map((s, i) => ({ s, i })).filter((x) => x.s.kind === "conflict");
  const done = blocks.filter((b) => choices[b.i]).length;
  const all = (c: Choice) => setChoices(Object.fromEntries(blocks.map((b) => [b.i, c])));
  const result = () =>
    file.segments
      .map((s, i) => (s.kind === "text" ? s.text : pick(s, choices[i] ?? "ours")))
      .join("");

  return (
    <div className={box}>
      {overview?.operation === "rebase" && (
        <p className="flex items-start gap-1.5 rounded-md bg-warning/10 px-2 py-1.5 text-fs-11 text-warning">
          <TriangleAlert size={12} className="mt-px shrink-0" />
          {t("work.conflict.rebaseHint")}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-2 text-fs-11 text-text-muted">
        <span>{t("work.conflict.progress", { done, n: blocks.length })}</span>
        <span className="ml-auto flex gap-1">
          <button
            type="button"
            onClick={() => all("ours")}
            className="rounded px-1.5 py-0.5 hover:bg-hover hover:text-text"
          >
            {t("work.conflict.allOurs")}
          </button>
          <button
            type="button"
            onClick={() => all("theirs")}
            className="rounded px-1.5 py-0.5 hover:bg-hover hover:text-text"
          >
            {t("work.conflict.allTheirs")}
          </button>
        </span>
      </div>
      {blocks.map(({ s, i }, n) =>
        s.kind === "conflict" ? (
          <Block
            key={i}
            index={n + 1}
            total={blocks.length}
            block={s}
            choice={choices[i] ?? null}
            onChoose={(c) => setChoices((old) => ({ ...old, [i]: c }))}
            compact={compact}
          />
        ) : null,
      )}
      <div className="flex items-center gap-2 border-t border-border pt-2">
        <span className="min-w-0 flex-1 text-fs-11 text-text-subtle">
          {done < blocks.length ? t("work.conflict.pickAll") : t("work.conflict.ready")}
        </span>
        <button
          type="button"
          disabled={busy || done < blocks.length}
          onClick={() => void run("work_resolve", { path, text: result() })}
          className="shrink-0 rounded-md bg-primary px-2.5 py-1 font-medium text-primary-fg hover:bg-primary-hover disabled:opacity-50"
        >
          {t("work.conflict.save")}
        </button>
      </div>
    </div>
  );
}

function Block({
  index,
  total,
  block,
  choice,
  onChoose,
  compact,
}: {
  index: number;
  total: number;
  block: Extract<Segment, { kind: "conflict" }>;
  choice: Choice | null;
  onChoose: (c: Choice) => void;
  compact: boolean;
}) {
  const t = useT();
  const side = (which: "ours" | "theirs") => {
    const chosen = choice === which || choice === "both" || choice === "bothTheirsFirst";
    const text = which === "ours" ? block.ours : block.theirs;
    const label = which === "ours" ? block.oursLabel : block.theirsLabel;
    return (
      <button
        type="button"
        onClick={() => onChoose(which)}
        className={`min-w-0 flex-1 rounded-md border text-left ${
          chosen
            ? which === "ours"
              ? "border-info bg-info/10"
              : "border-success bg-success/10"
            : choice
              ? "border-border opacity-50 hover:opacity-100"
              : "border-border hover:border-border-strong"
        }`}
      >
        <div
          className={`flex items-center gap-1 border-b border-border px-2 py-1 text-fs-11 font-medium ${
            which === "ours" ? "text-info" : "text-success"
          }`}
        >
          {chosen && <Check size={12} />}
          {t(which === "ours" ? "work.conflict.ours" : "work.conflict.theirs")}
          <span className="min-w-0 truncate font-normal text-text-subtle">{label}</span>
        </div>
        <pre className="max-h-48 overflow-auto px-2 py-1 font-mono text-fs-11 leading-snug whitespace-pre-wrap">
          {text || <span className="text-text-subtle">{t("work.conflict.empty")}</span>}
        </pre>
      </button>
    );
  };
  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-2 text-fs-11">
        <span className="font-semibold">{t("work.conflict.block", { n: index, total })}</span>
        <span className="ml-auto flex gap-1">
          <button
            type="button"
            onClick={() => onChoose("both")}
            className={`rounded px-1.5 py-0.5 ${choice === "both" ? "bg-active font-medium" : "text-text-muted hover:bg-hover"}`}
          >
            {t("work.conflict.both")}
          </button>
          <button
            type="button"
            onClick={() => onChoose("bothTheirsFirst")}
            className={`rounded px-1.5 py-0.5 ${choice === "bothTheirsFirst" ? "bg-active font-medium" : "text-text-muted hover:bg-hover"}`}
          >
            {t("work.conflict.bothTheirsFirst")}
          </button>
        </span>
      </div>
      <div className={`flex gap-2 ${compact ? "flex-col" : ""}`}>
        {side("ours")}
        {side("theirs")}
      </div>
    </div>
  );
}
