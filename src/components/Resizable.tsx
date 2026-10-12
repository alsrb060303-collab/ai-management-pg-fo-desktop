import { useCallback, useRef, useState } from "react";
import { useT } from "../i18n";

const PREFIX = "panel-width:";

function readWidth(key: string, fallback: number, min: number, max: number): number {
  try {
    const value = Number(localStorage.getItem(PREFIX + key));
    if (Number.isFinite(value) && value > 0) return Math.min(max, Math.max(min, value));
  } catch {
    // Only a convenience.
  }
  return fallback;
}

/**
 * A panel width the user can drag, remembered per `key`. `dragging` is true
 * while the handle is held (turn width transitions off then).
 */
export function usePanelWidth(key: string, initial: number, min: number, max: number) {
  const [width, setWidth] = useState(() => readWidth(key, initial, min, max));
  const [dragging, setDragging] = useState(false);
  const clamp = useCallback((w: number) => Math.min(max, Math.max(min, w)), [min, max]);
  const save = useCallback(
    (w: number) => {
      try {
        localStorage.setItem(PREFIX + key, String(Math.round(w)));
      } catch {
        // Only a convenience.
      }
    },
    [key],
  );
  return {
    width,
    dragging,
    handle: {
      width,
      onResize: (w: number) => setWidth(clamp(w)),
      onStart: () => setDragging(true),
      onEnd: (w: number) => {
        setDragging(false);
        save(clamp(w));
      },
      onReset: () => {
        setWidth(initial);
        save(initial);
      },
    },
  };
}

/**
 * The draggable edge of a panel. `side` is the edge it sits on: a handle on
 * the right edge grows the panel when dragged right, one on the left edge
 * when dragged left. Double-click restores the default width. Pointer
 * events, not HTML5 drag and drop (which WebKitGTK breaks).
 */
export function ResizeHandle({
  side,
  width,
  onResize,
  onStart,
  onEnd,
  onReset,
}: {
  side: "left" | "right";
  width: number;
  onResize: (width: number) => void;
  onStart: () => void;
  onEnd: (width: number) => void;
  onReset: () => void;
}) {
  const t = useT();
  const start = useRef<{ x: number; width: number; last: number } | null>(null);
  const next = (clientX: number) => {
    const s = start.current;
    if (!s) return width;
    const delta = clientX - s.x;
    return side === "right" ? s.width + delta : s.width - delta;
  };
  return (
    // biome-ignore lint/a11y/useSemanticElements: a vertical splitter, not a form separator
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={t("common.resize")}
      aria-valuenow={Math.round(width)}
      title={t("common.resizeHint")}
      tabIndex={0}
      onKeyDown={(e) => {
        const step = e.key === "ArrowRight" ? 16 : e.key === "ArrowLeft" ? -16 : 0;
        if (!step) return;
        e.preventDefault();
        onEnd(width + (side === "right" ? step : -step));
        onResize(width + (side === "right" ? step : -step));
      }}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        start.current = { x: e.clientX, width, last: width };
        onStart();
      }}
      onPointerMove={(e) => {
        if (!start.current) return;
        start.current.last = next(e.clientX);
        onResize(start.current.last);
      }}
      onPointerUp={(e) => {
        if (!start.current) return;
        const w = next(e.clientX);
        start.current = null;
        onEnd(w);
      }}
      onPointerCancel={() => {
        const s = start.current;
        start.current = null;
        if (s) onEnd(s.last);
      }}
      onDoubleClick={onReset}
      className={`absolute top-0 bottom-0 z-20 w-1.5 cursor-col-resize touch-none transition-colors hover:bg-accent/40 active:bg-accent/60 ${
        side === "right" ? "-right-[3px]" : "-left-[3px]"
      }`}
    />
  );
}
