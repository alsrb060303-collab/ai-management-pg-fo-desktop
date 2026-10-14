/**
 * Lane assignment for the work trees. Pure functions: rows go in newest first,
 * and each row comes back with what its gutter draws (a dot plus line pieces).
 */

/** What a piece of line covers in a row: all of it, down to the dot, or from it. */
export type SegPart = "full" | "above" | "below";

export interface Seg {
  lane: number;
  part: SegPart;
  /** Color key, chosen by the caller. */
  tone: string;
  /** For `below`: the line leans into this lane at the bottom of the row. */
  bendTo?: number;
}

export interface GutterRow {
  /** Lane of this row's dot. */
  lane: number;
  segs: Seg[];
}

export interface Layout {
  lanes: number;
  gutter: GutterRow[];
}

interface Interval {
  key: string;
  top: number;
  bottom: number;
}

/**
 * Greedy interval coloring: each interval gets the lowest lane whose previous
 * occupant ended above it. `pinned` keys get lane 0 and nobody else uses it.
 */
export function assignLanes(intervals: Interval[], pinned?: string): Map<string, number> {
  const lanes = new Map<string, number>();
  const ends: number[] = [];
  const first = pinned && intervals.some((i) => i.key === pinned) ? 1 : 0;
  if (first === 1) ends[0] = Number.POSITIVE_INFINITY;
  if (pinned && first === 1) lanes.set(pinned, 0);
  const rest = intervals
    .filter((i) => !(first === 1 && i.key === pinned))
    .sort((a, b) => a.top - b.top || a.bottom - b.bottom);
  for (const iv of rest) {
    let lane = first;
    while (lane < ends.length && ends[lane] >= iv.top) lane++;
    ends[lane] = iv.bottom;
    lanes.set(iv.key, lane);
  }
  return lanes;
}

export interface GraphRow {
  /** Commit hash; null for a row that is not a commit (no lines). */
  sha: string | null;
  parents: string[];
  /** Branch the row belongs to. */
  group: string;
  tone: string;
}

/**
 * Lanes for the git graph. One lane per branch, `trunk` (the default branch)
 * on the left. A line runs from each commit down to its parent: it bends into
 * the parent's lane at the bottom of the child's row (a fork when the child is
 * the oldest commit of a branch, a merge when the parent is the branch tip),
 * then continues straight down to the parent.
 */
export function layoutGraph(rows: GraphRow[], trunk: string | null): Layout {
  const rowOf = new Map<string, number>();
  rows.forEach((r, i) => {
    if (r.sha && !rowOf.has(r.sha)) rowOf.set(r.sha, i);
  });

  // Edges to parents that are shown further down.
  const edges: { child: number; parent: number }[] = [];
  rows.forEach((r, child) => {
    if (!r.sha) return;
    for (const p of r.parents) {
      const parent = rowOf.get(p);
      if (parent !== undefined && parent > child) edges.push({ child, parent });
    }
  });

  const span = new Map<string, Interval>();
  const touch = (key: string, row: number) => {
    const iv = span.get(key);
    if (!iv) span.set(key, { key, top: row, bottom: row });
    else {
      iv.top = Math.min(iv.top, row);
      iv.bottom = Math.max(iv.bottom, row);
    }
  };
  rows.forEach((r, i) => {
    if (r.sha) touch(r.group, i);
  });
  for (const e of edges) touch(rows[e.parent].group, e.child);

  const laneOf = assignLanes([...span.values()], trunk ?? undefined);
  const lane = (group: string) => laneOf.get(group) ?? 0;

  const gutter: GutterRow[] = rows.map((r) => ({ lane: lane(r.group), segs: [] }));
  for (const { child, parent } of edges) {
    const from = lane(rows[child].group);
    const to = lane(rows[parent].group);
    const tone = rows[child].tone;
    gutter[child].segs.push({
      lane: from,
      part: "below",
      tone,
      bendTo: to === from ? undefined : to,
    });
    for (let r = child + 1; r < parent; r++) gutter[r].segs.push({ lane: to, part: "full", tone });
    gutter[parent].segs.push({ lane: to, part: "above", tone });
  }
  return { lanes: Math.max(1, ...gutter.map((g) => g.lane + 1)), gutter };
}

export interface SessionRow {
  session: string;
  tone: string;
}

/**
 * Lanes for the prompt tree: one lane per session, so sessions that overlap in
 * time sit side by side. A session's line spans its first to last prompt.
 */
export function layoutSessions(rows: SessionRow[]): Layout {
  const span = new Map<string, Interval>();
  rows.forEach((r, i) => {
    const iv = span.get(r.session);
    if (!iv) span.set(r.session, { key: r.session, top: i, bottom: i });
    else iv.bottom = i;
  });
  const laneOf = assignLanes([...span.values()]);
  const gutter: GutterRow[] = rows.map((r) => ({ lane: laneOf.get(r.session) ?? 0, segs: [] }));
  rows.forEach((r, i) => {
    const iv = span.get(r.session);
    if (!iv) return;
    const lane = gutter[i].lane;
    if (i > iv.top) gutter[i].segs.push({ lane, part: "above", tone: r.tone });
    if (i < iv.bottom) gutter[i].segs.push({ lane, part: "below", tone: r.tone });
  });
  // Lines of other sessions pass through rows between their prompts.
  for (const iv of span.values()) {
    const tone = rows[iv.top].tone;
    const lane = gutter[iv.top].lane;
    for (let i = iv.top + 1; i < iv.bottom; i++) {
      if (rows[i].session !== iv.key) gutter[i].segs.push({ lane, part: "full", tone });
    }
  }
  return { lanes: Math.max(1, ...gutter.map((g) => g.lane + 1)), gutter };
}

/** Commits reachable from the given tips, following the parents we know. */
export function reachableFrom(
  tips: (string | null | undefined)[],
  parentsOf: Map<string, string[]>,
): Set<string> {
  const seen = new Set<string>();
  const stack = tips.filter((t): t is string => !!t);
  while (stack.length) {
    const sha = stack.pop() as string;
    if (seen.has(sha)) continue;
    seen.add(sha);
    for (const p of parentsOf.get(sha) ?? []) stack.push(p);
  }
  return seen;
}
