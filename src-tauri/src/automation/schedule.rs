//! When an automation runs next, in local time.

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};

/// The shortest interval between runs.
pub const MIN_INTERVAL_MINUTES: u32 = 5;
/// A run missed while the app was closed still happens once within this long.
pub const CATCH_UP: Duration = Duration::hours(24);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Schedule {
    /// At `time` ("HH:MM") on `days` (0 = Sunday … 6 = Saturday; empty = every day).
    Daily {
        time: String,
        #[serde(default)]
        days: Vec<u8>,
    },
    /// Every `minutes` minutes.
    Interval { minutes: u32 },
}

impl Schedule {
    pub fn check(&self) -> Result<(), String> {
        match self {
            Schedule::Daily { time, days } => {
                parse_time(time).ok_or_else(|| {
                    crate::tr!(
                        "시간은 HH:MM 형식으로 적어 주세요: {time}",
                        "Write the time as HH:MM: {time}"
                    )
                })?;
                if days.iter().any(|d| *d > 6) {
                    return Err(crate::tr!(
                        "요일은 0(일)부터 6(토)까지예요",
                        "Days go from 0 (Sunday) to 6 (Saturday)"
                    ));
                }
                Ok(())
            }
            Schedule::Interval { minutes } if *minutes < MIN_INTERVAL_MINUTES => Err(crate::tr!(
                "간격은 {MIN_INTERVAL_MINUTES}분 이상이어야 해요",
                "The interval must be at least {MIN_INTERVAL_MINUTES} minutes"
            )),
            Schedule::Interval { .. } => Ok(()),
        }
    }

    /// The first run strictly after `after`.
    pub fn next_after<Tz: TimeZone>(&self, after: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        match self {
            Schedule::Interval { minutes } => {
                Some(after.clone() + Duration::minutes(i64::from((*minutes).max(1))))
            }
            Schedule::Daily { time, days } => {
                let at = parse_time(time)?;
                let tz = after.timezone();
                let start = after.date_naive();
                (0..=8).find_map(|offset| {
                    let date = start + Duration::days(offset);
                    let day = date.weekday().num_days_from_sunday() as u8;
                    if !days.is_empty() && !days.contains(&day) {
                        return None;
                    }
                    // A time skipped by a clock change runs at the first valid one after it.
                    let candidate = tz
                        .from_local_datetime(&date.and_time(at))
                        .earliest()
                        .or_else(|| {
                            tz.from_local_datetime(&date.and_time(at + Duration::hours(1)))
                                .earliest()
                        })?;
                    (candidate > *after).then_some(candidate)
                })
            }
        }
    }
}

fn parse_time(text: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(text.trim(), "%H:%M").ok()
}

pub fn from_ms(ms: i64) -> DateTime<Local> {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Local::now)
}

/// What to do now for an automation last run (or edited) at `base`.
#[derive(Debug, PartialEq, Eq)]
pub enum Due {
    No,
    Run,
    /// Missed long ago (the app was closed): skip to the next time.
    Skip,
}

pub fn due<Tz: TimeZone>(schedule: &Schedule, base: &DateTime<Tz>, now: &DateTime<Tz>) -> Due {
    match schedule.next_after(base) {
        Some(next) if next <= *now => {
            if now.clone() - next <= CATCH_UP {
                Due::Run
            } else {
                Due::Skip
            }
        }
        _ => Due::No,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    fn at(text: &str) -> DateTime<FixedOffset> {
        // 2026-10-15 is a Thursday.
        FixedOffset::east_opt(9 * 3600)
            .unwrap()
            .from_local_datetime(
                &chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap(),
            )
            .unwrap()
    }

    fn daily(time: &str, days: &[u8]) -> Schedule {
        Schedule::Daily {
            time: time.into(),
            days: days.to_vec(),
        }
    }

    #[test]
    fn daily_runs_today_if_the_time_is_ahead_else_tomorrow() {
        let s = daily("10:00", &[]);
        assert_eq!(
            s.next_after(&at("2026-10-15 09:00")),
            Some(at("2026-10-15 10:00"))
        );
        assert_eq!(
            s.next_after(&at("2026-10-15 10:00")),
            Some(at("2026-10-16 10:00"))
        );
    }

    #[test]
    fn weekdays_skip_the_weekend() {
        let s = daily("10:00", &[1, 2, 3, 4, 5]);
        // Friday after ten → Monday.
        assert_eq!(
            s.next_after(&at("2026-10-16 11:00")),
            Some(at("2026-10-19 10:00"))
        );
    }

    #[test]
    fn intervals_and_checks() {
        let s = Schedule::Interval { minutes: 30 };
        assert_eq!(
            s.next_after(&at("2026-10-15 09:00")),
            Some(at("2026-10-15 09:30"))
        );
        assert!(Schedule::Interval { minutes: 1 }.check().is_err());
        assert!(daily("25:00", &[]).check().is_err());
        assert!(daily("10:00", &[7]).check().is_err());
        assert!(daily("09:05", &[0, 6]).check().is_ok());
    }

    #[test]
    fn missed_runs_catch_up_once_within_a_day() {
        let s = daily("10:00", &[]);
        let base = at("2026-10-15 09:00");
        assert_eq!(due(&s, &base, &at("2026-10-15 09:59")), Due::No);
        assert_eq!(due(&s, &base, &at("2026-10-15 10:00")), Due::Run);
        assert_eq!(due(&s, &base, &at("2026-10-16 09:00")), Due::Run);
        assert_eq!(due(&s, &base, &at("2026-10-17 11:00")), Due::Skip);
    }

    #[test]
    fn schedules_read_the_frontends_json() {
        let s: Schedule =
            serde_json::from_str(r#"{"kind":"daily","time":"10:00","days":[1,2]}"#).unwrap();
        assert_eq!(s, daily("10:00", &[1, 2]));
        let s: Schedule = serde_json::from_str(r#"{"kind":"interval","minutes":60}"#).unwrap();
        assert_eq!(s, Schedule::Interval { minutes: 60 });
    }
}
