import type { ReactNode } from "react";
import type { GutterRow, Seg } from "./layout";

/** Distance from the top of a row to the center of its dot. */
const DOT_Y = 11;
/** Height of the sloped piece where a line moves to another lane. */
const BEND = 16;

/** Line colors by tone; full class names so Tailwind finds them. */
const LINE: Record<string, { bg: string; text: string }> = {
  pushed: { bg: "bg-success/50", text: "text-success/50" },
  local: { bg: "bg-accent/50", text: "text-accent/50" },
  merged: { bg: "bg-merged/60", text: "text-merged/60" },
  claude: { bg: "bg-accent/50", text: "text-accent/50" },
  codex: { bg: "bg-info/50", text: "text-info/50" },
};
const FALLBACK = { bg: "bg-border-strong", text: "text-border-strong" };

export function laneWidth(compact: boolean) {
  return compact ? 14 : 20;
}

/** The lines and dot of one row. Sits left of the row's text, as tall as it. */
export function Rail({
  row,
  lanes,
  compact,
  dot,
}: {
  row: GutterRow;
  lanes: number;
  compact: boolean;
  dot: ReactNode;
}) {
  const lw = laneWidth(compact);
  const x = (lane: number) => lane * lw + lw / 2;
  return (
    <div aria-hidden className="relative shrink-0 self-stretch" style={{ width: lanes * lw }}>
      {unique(row.segs).map((seg) => (
        <Line key={`${seg.lane}:${seg.part}:${seg.bendTo ?? ""}`} seg={seg} x={x} />
      ))}
      <span
        className="absolute z-10 flex -translate-x-1/2 -translate-y-1/2 items-center justify-center"
        style={{ left: x(row.lane), top: DOT_Y }}
      >
        {dot}
      </span>
    </div>
  );
}

/** Two lines in the same place (a shared stretch) are drawn once. */
function unique(segs: Seg[]): Seg[] {
  const seen = new Set<string>();
  return segs.filter((s) => {
    const key = `${s.lane}:${s.part}:${s.bendTo ?? ""}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function Line({ seg, x }: { seg: Seg; x: (lane: number) => number }) {
  const color = LINE[seg.tone] ?? FALLBACK;
  const bends = seg.part === "below" && seg.bendTo !== undefined;
  const style =
    seg.part === "full"
      ? { top: 0, bottom: 0 }
      : seg.part === "above"
        ? { top: 0, height: DOT_Y }
        : { top: DOT_Y, bottom: bends ? BEND : 0 };
  return (
    <>
      <span
        className={`absolute w-0.5 -translate-x-1/2 ${color.bg}`}
        style={{ left: x(seg.lane), ...style }}
      />
      {bends && seg.bendTo !== undefined && (
        <Bend from={x(seg.lane)} to={x(seg.bendTo)} className={color.text} />
      )}
    </>
  );
}

function Bend({ from, to, className }: { from: number; to: number; className: string }) {
  const left = Math.min(from, to) - 1;
  const width = Math.abs(to - from) + 2;
  const a = from - left;
  const b = to - left;
  return (
    <svg
      className={`absolute bottom-0 overflow-visible ${className}`}
      style={{ left }}
      width={width}
      height={BEND}
      viewBox={`0 0 ${width} ${BEND}`}
      fill="none"
      role="presentation"
    >
      <path
        d={`M ${a} 0 C ${a} ${BEND / 2}, ${b} ${BEND / 2}, ${b} ${BEND}`}
        stroke="currentColor"
        strokeWidth={2}
      />
    </svg>
  );
}
