use std::path::PathBuf;

pub const ARCHIVE_FILE_NAME: &str = "madrigal-backup.json";
pub const DAY_SECS: i64 = 86_400;
pub const WEEK_SECS: i64 = 7 * DAY_SECS;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    IntervalSecs(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverwritePolicy {
    Overwrite,
    KeepAll,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScheduleConfig {
    pub frequency: Frequency,
    pub destination_dir: PathBuf,
    pub overwrite: OverwritePolicy,
    pub last_run: Option<i64>,
    #[serde(default)]
    pub redownload_missing: bool,
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        _ => 28,
    }
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted = if month <= 2 { year - 1 } else { year };
    let era = adjusted.div_euclid(400);
    let yoe = adjusted - era * 400;
    let mp = (month + 9).rem_euclid(12);
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn add_one_month(stamp: i64) -> i64 {
    let days = stamp.div_euclid(DAY_SECS);
    let within = stamp.rem_euclid(DAY_SECS);
    let (year, month, day) = civil_from_days(days);
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let clamped = day.min(days_in_month(next_year, next_month));
    days_from_civil(next_year, next_month, clamped) * DAY_SECS + within
}

fn period_after(frequency: &Frequency, base: i64) -> i64 {
    match frequency {
        Frequency::Daily => base + DAY_SECS,
        Frequency::Weekly => base + WEEK_SECS,
        Frequency::Monthly => add_one_month(base),
        Frequency::IntervalSecs(secs) => base + (*secs as i64).max(1),
    }
}

pub fn next_run(config: &ScheduleConfig, now: i64) -> i64 {
    period_after(&config.frequency, config.last_run.unwrap_or(now))
}

pub fn is_due(config: &ScheduleConfig, now: i64) -> bool {
    now >= next_run(config, now)
}

fn stamp_text(stamp: i64) -> String {
    let days = stamp.div_euclid(DAY_SECS);
    let within = stamp.rem_euclid(DAY_SECS);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        within / 3600,
        (within % 3600) / 60,
        within % 60
    )
}

pub fn destination_for(config: &ScheduleConfig, now: i64) -> PathBuf {
    match config.overwrite {
        OverwritePolicy::Overwrite => config.destination_dir.join(ARCHIVE_FILE_NAME),
        OverwritePolicy::KeepAll => config
            .destination_dir
            .join(format!("madrigal-backup-{}.json", stamp_text(now))),
    }
}

pub fn poll(config: &ScheduleConfig, now: i64) -> Option<PathBuf> {
    if is_due(config, now) {
        Some(destination_for(config, now))
    } else {
        None
    }
}

pub fn mark_ran(config: &mut ScheduleConfig, now: i64) {
    config.last_run = Some(now);
}
