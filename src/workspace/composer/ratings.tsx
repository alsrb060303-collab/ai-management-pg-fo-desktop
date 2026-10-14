import { Panel } from "@xyflow/react";
import { Gauge } from "lucide-react";
import { useMemo } from "react";
import { type MessageKey, useT } from "../../i18n";
import { type Rating, useComposerStore } from "../../store/composer";

type Axis = keyof Rating;

const AXES: { axis: Axis; label: MessageKey }[] = [
  { axis: "tokens", label: "composer.rating.tokens" },
  { axis: "performance", label: "composer.rating.performance" },
  { axis: "quality", label: "composer.rating.quality" },
];

/** Fewer tokens is the good direction; more performance and quality are. */
const goodWhenUp = (axis: Axis) => axis !== "tokens";

const signed = (n: number) => (n > 0 ? `+${n}` : `${n}`);

/** One line under a palette block: `토큰 ↓ 성능 ↑↑ 품질 ·`. */
export function RatingBadges({ rating }: { rating: Rating }) {
  const t = useT();
  return (
    <div className="mt-0.5 flex items-center gap-2 text-fs-10" title={t("composer.rating.hint")}>
      {AXES.map(({ axis, label }) => {
        const v = rating[axis];
        const good = v !== 0 && v > 0 === goodWhenUp(axis);
        return (
          <span key={axis} className="flex items-center gap-0.5 text-text-subtle">
            {t(label)}
            <span
              className={`font-semibold ${v === 0 ? "" : good ? "text-success" : "text-danger"}`}
            >
              {v === 0 ? "·" : (v > 0 ? "↑" : "↓").repeat(Math.abs(v))}
            </span>
          </span>
        );
      })}
    </div>
  );
}

/** Sums the ratings of the blocks switched on in the canvas, each block once. */
function useTotals() {
  const nodes = useComposerStore((s) => s.doc?.nodes);
  const blocks = useComposerStore((s) => s.blocks);
  return useMemo(() => {
    const ids = new Set<string>();
    for (const node of nodes ?? []) {
      if (node.type !== "block" || node.data.disabled) continue;
      ids.add(node.data.blockId);
    }
    const sum = { tokens: 0, performance: 0, quality: 0 };
    let rated = 0;
    for (const id of ids) {
      const rating = blocks.get(id)?.rating;
      if (!rating) continue;
      rated += 1;
      sum.tokens += rating.tokens;
      sum.performance += rating.performance;
      sum.quality += rating.quality;
    }
    return { sum, rated, unrated: ids.size - rated };
  }, [nodes, blocks]);
}

/** Top-right of the canvas: performance, quality and token use as diverging bars. */
export function RatingPanel() {
  const t = useT();
  const { sum, rated, unrated } = useTotals();
  // One scale for all three bars: ±15, so it takes many blocks to fill one.
  const scale = Math.max(
    15,
    Math.abs(sum.performance),
    Math.abs(sum.quality),
    Math.abs(sum.tokens),
  );
  const bars: {
    label: MessageKey;
    value: number;
    good: boolean;
    hint?: MessageKey;
  }[] = [
    { label: "composer.rating.performance", value: sum.performance, good: true },
    { label: "composer.rating.quality", value: sum.quality, good: true },
    {
      label: "composer.rating.tokenUse",
      value: sum.tokens,
      good: false,
      hint: "composer.rating.tokenUseHint",
    },
  ];

  return (
    <Panel position="top-right" className="!m-3">
      <div className="w-60 rounded-xl border border-border bg-surface-raised/95 p-2.5 text-fs-11 shadow-card backdrop-blur">
        <div className="mb-1.5 flex items-center gap-1.5">
          <Gauge size={13} className="text-text-muted" />
          <span className="font-semibold">{t("composer.rating.title")}</span>
        </div>
        {rated === 0 ? (
          <p className="text-text-subtle">{t("composer.rating.empty")}</p>
        ) : (
          <div className="space-y-1">
            {bars.map((bar) => {
              const width = (Math.abs(bar.value) / scale) * 50;
              const good = bar.value > 0 === bar.good;
              const label = t(bar.label);
              return (
                <div
                  key={bar.label}
                  className="flex items-center gap-2"
                  title={[
                    t("composer.rating.bar", { label, value: signed(bar.value) }),
                    bar.hint && t(bar.hint),
                  ]
                    .filter(Boolean)
                    .join("\n")}
                >
                  <span className="w-[4.75rem] shrink-0 truncate text-text-muted">{label}</span>
                  <div className="relative h-2 flex-1 rounded-full bg-sunken">
                    <div className="absolute inset-y-0 left-1/2 w-px bg-border-strong" />
                    {bar.value !== 0 && (
                      <div
                        className={`absolute inset-y-0 transition-all duration-300 ${
                          bar.value > 0 ? "left-1/2 rounded-r-full" : "right-1/2 rounded-l-full"
                        } ${good ? "bg-success" : "bg-danger"}`}
                        style={{ width: `${width}%` }}
                      />
                    )}
                  </div>
                  <span className="w-6 shrink-0 text-right tabular-nums">{signed(bar.value)}</span>
                </div>
              );
            })}
          </div>
        )}
        {(rated > 0 || unrated > 0) && (
          <p className="mt-1.5 text-fs-10 text-text-subtle">
            {t("composer.rating.count", { n: rated })}
            {unrated > 0 && ` · ${t("composer.rating.unrated", { n: unrated })}`}
          </p>
        )}
      </div>
    </Panel>
  );
}
