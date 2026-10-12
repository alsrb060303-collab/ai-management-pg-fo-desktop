import { TriangleAlert } from "lucide-react";
import { useState } from "react";
import { Button } from "../../components/ui";
import { useT } from "../../i18n";
import type { Block } from "../../store/composer";

/**
 * A block gets a warning before it's added when its own text (read whenever
 * Claude loads it) or everything it writes (with reference files read only
 * when needed) is above these many bytes.
 */
const LARGE_BODY = 15_000;
const LARGE_TOTAL = 40_000;
const OFF_KEY = "composer.largeBlockWarning.off";

export function isLarge(block: Block): boolean {
  return new TextEncoder().encode(block.body).length > LARGE_BODY || block.size > LARGE_TOTAL;
}

export function largeWarningOff(): boolean {
  try {
    return localStorage.getItem(OFF_KEY) === "1";
  } catch {
    return false;
  }
}

function turnWarningOff() {
  try {
    localStorage.setItem(OFF_KEY, "1");
  } catch {
    // Only a convenience: the warning shows again next time.
  }
}

const kb = (bytes: number) => Math.max(1, Math.round(bytes / 1024));

/** Asks before a large block goes on the canvas, with "don't show again". */
export function LargeBlockDialog({
  block,
  onAdd,
  onCancel,
}: {
  block: Block;
  onAdd: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  const [off, setOff] = useState(false);
  const files = block.companions.length;
  const body = new TextEncoder().encode(block.body).length;
  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/30 p-6"
      onPointerDown={(e) => e.target === e.currentTarget && onCancel()}
    >
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="large-block-title"
        className="w-full max-w-md rounded-2xl border border-border bg-surface-raised p-5 shadow-card"
      >
        <h2 id="large-block-title" className="flex items-center gap-2 font-medium">
          <TriangleAlert size={17} className="text-warning" />
          {t("composer.large.title")}
        </h2>
        <p className="mt-3 text-sm leading-relaxed">
          {t("composer.large.body", { title: block.title, kb: kb(body) })}
        </p>
        {files > 0 && (
          <p className="mt-2 text-sm leading-relaxed text-text-muted">
            {t("composer.large.files", { n: files, kb: kb(Math.max(0, block.size - body)) })}
          </p>
        )}
        <label className="mt-4 flex items-center gap-2 text-sm text-text-muted">
          <input type="checkbox" checked={off} onChange={(e) => setOff(e.target.checked)} />
          {t("composer.large.dontShow")}
        </label>
        <div className="mt-4 flex justify-end gap-2">
          <Button size="sm" onClick={onCancel}>
            {t("composer.large.cancel")}
          </Button>
          <Button
            size="sm"
            variant="primary"
            autoFocus
            onClick={() => {
              if (off) turnWarningOff();
              onAdd();
            }}
          >
            {t("composer.large.add")}
          </Button>
        </div>
      </div>
    </div>
  );
}
