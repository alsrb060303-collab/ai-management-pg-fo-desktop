import { useT } from "../../i18n";
import type { Mode } from "../../store/library";

export const smallButton =
  "shrink-0 rounded border border-border px-1.5 py-0.5 text-fs-11 hover:border-border-strong disabled:opacity-50";

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
  const t = useT();
  return (
    <>
      <button
        type="button"
        className={smallButton}
        disabled={disabled}
        title={t("docs.mode.personalHint")}
        onClick={() => onPick("personal")}
      >
        {label} · {t("docs.mode.personal")}
      </button>
      {!personalOnly && (
        <button
          type="button"
          className={smallButton}
          disabled={disabled}
          title={t("docs.mode.teamHint")}
          onClick={() => onPick("team")}
        >
          {label} · {t("docs.mode.team")}
        </button>
      )}
    </>
  );
}
