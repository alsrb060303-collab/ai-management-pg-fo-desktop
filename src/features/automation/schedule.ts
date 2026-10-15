import { intlLocale, type MessageKey, t } from "../../i18n";
import type { Schedule } from "../../store/automation";

const DAY_KEYS: MessageKey[] = [
  "auto.day.0",
  "auto.day.1",
  "auto.day.2",
  "auto.day.3",
  "auto.day.4",
  "auto.day.5",
  "auto.day.6",
];

export const dayLabel = (day: number) => t(DAY_KEYS[day]);

/** "매일 10:00", "평일 09:30", "월·수 10:00", "2시간마다". */
export function describeSchedule(s: Schedule): string {
  if (s.kind === "interval") {
    return s.minutes % 60 === 0
      ? t("auto.schedule.hours", { n: s.minutes / 60 })
      : t("auto.schedule.interval", { n: s.minutes });
  }
  const days = [...s.days].sort();
  const same = (list: number[]) =>
    days.length === list.length && list.every((d, i) => days[i] === d);
  if (days.length === 0 || days.length === 7) return t("auto.schedule.daily", { time: s.time });
  if (same([1, 2, 3, 4, 5])) return t("auto.schedule.weekdays", { time: s.time });
  if (same([0, 6])) return t("auto.schedule.weekend", { time: s.time });
  return t("auto.schedule.days", { days: days.map(dayLabel).join("·"), time: s.time });
}

/** A short local date and time, e.g. "10/15 10:00". */
export function when(ms: number): string {
  return new Date(ms).toLocaleString(intlLocale(), {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
