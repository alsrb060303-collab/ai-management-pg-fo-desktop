import { useT } from "../../i18n";
import type { Diff } from "../../store/work";

type Row = { kind: "file"; path: string } | { kind: "line"; text: string };

/** `--- a/x` + `+++ b/x` pairs become one file heading. */
function rows(text: string): Row[] {
  const lines = text.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  const out: Row[] = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const next = lines[i + 1];
    if (line.startsWith("--- ") && next?.startsWith("+++ ")) {
      const plus = next.slice(4);
      const path = plus === "/dev/null" ? line.slice(4) : plus;
      out.push({ kind: "file", path: path.replace(/^[ab]\//, "") });
      i++;
    } else {
      out.push({ kind: "line", text: line });
    }
  }
  return out;
}

const META =
  /^(new file mode|deleted file mode|old mode|new mode|similarity index|rename from|rename to|copy from|copy to)/;

/** A unified diff, coloured by line, with a heading per file. */
export function DiffView({ diff, compact }: { diff: Diff | null; compact: boolean }) {
  const t = useT();
  if (!diff)
    return <p className="px-2 py-2 text-fs-11 text-text-subtle">{t("work.diff.loading")}</p>;
  if (diff.binary && !diff.text.trim())
    return <p className="px-2 py-2 text-fs-11 text-text-subtle">{t("work.diff.binary")}</p>;
  const list = rows(diff.text);
  return (
    <div
      className={`selectable my-1 overflow-auto rounded-lg border border-border bg-sunken font-mono leading-snug ${
        compact ? "max-h-72 text-fs-10.5" : "max-h-[28rem] text-xs"
      }`}
    >
      <pre className="min-w-fit py-1">
        {list.map((row, i) =>
          row.kind === "file" ? (
            <div
              // biome-ignore lint/suspicious/noArrayIndexKey: lines have no identity
              key={i}
              className="mt-1 border-y border-border bg-surface-raised px-2 py-0.5 font-sans font-semibold text-text first:mt-0 first:border-t-0"
            >
              {row.path}
            </div>
          ) : (
            <div
              // biome-ignore lint/suspicious/noArrayIndexKey: lines have no identity
              key={i}
              className={`px-2 ${
                row.text.startsWith("@@")
                  ? "text-info"
                  : META.test(row.text) || row.text.startsWith("Binary files")
                    ? "text-text-subtle"
                    : row.text.startsWith("+")
                      ? "bg-success/10 text-success"
                      : row.text.startsWith("-")
                        ? "bg-danger/10 text-danger"
                        : "text-text-muted"
              }`}
            >
              {row.text || " "}
            </div>
          ),
        )}
        {list.length === 0 && <div className="px-2 text-text-subtle">{t("work.diff.empty")}</div>}
      </pre>
      {diff.truncated && (
        <p className="border-t border-border px-2 py-1 text-fs-11 text-text-subtle">
          {t("work.diff.truncated")}
        </p>
      )}
    </div>
  );
}
