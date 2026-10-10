import type { Mode } from "../../store/library";

export const smallButton =
  "shrink-0 rounded border border-border px-1.5 py-0.5 text-[11px] hover:border-text-muted disabled:opacity-50";

/** A pair of buttons for the personal ("나만") and team ("팀") variants of an action. */
export function ModeButtons({
  label,
  disabled,
  personalOnly,
  onPick,
}: {
  label: string;
  disabled?: boolean;
  /** Hides the team variant, e.g. for items that only exist on this machine. */
  personalOnly?: boolean;
  onPick: (mode: Mode) => void;
}) {
  return (
    <>
      <button
        type="button"
        className={smallButton}
        disabled={disabled}
        title="git에 올리지 않고 나만 사용"
        onClick={() => onPick("personal")}
      >
        {label} · 나만
      </button>
      {!personalOnly && (
        <button
          type="button"
          className={smallButton}
          disabled={disabled}
          title="커밋해서 팀과 공유"
          onClick={() => onPick("team")}
        >
          {label} · 팀
        </button>
      )}
    </>
  );
}
