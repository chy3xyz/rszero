//! Cron expression scheduler — supports standard 5-field cron syntax.
//!
//! Parses `* * * * *` (minute hour day month weekday) expressions and
//! calculates the next execution time. Integrates with [`Scheduler`] for
//! cron-based periodic tasks.
//!
//! # Example
//!
//! ```no_run
//! use rszero::scheduler::cron::{CronExpr, CronScheduler};
//!
//! # async fn example() {
//! let cron = CronExpr::parse("0 9 * * 1-5").unwrap(); // 9am weekdays
//! let sched = CronScheduler::new();
//! sched.cron(cron, || async {
//!     tracing::info!("good morning, weekday!");
//! }).await;
//! # }
//! ```

use crate::error::{RszeroError, RszeroResult};
use super::JobHandle;
use std::future::Future;
use std::time::Duration;
use chrono::{Timelike, Datelike};
use tokio_util::sync::CancellationToken;

/// Parsed cron expression with 5 fields: minute hour day month weekday.
#[derive(Debug, Clone)]
pub struct CronExpr {
    minutes: Vec<u8>,
    hours: Vec<u8>,
    days: Vec<u8>,
    months: Vec<u8>,
    weekdays: Vec<u8>,
}

impl CronExpr {
    /// Parse a 5-field cron expression.
    ///
    /// Supported formats:
    /// - `*` (any)
    /// - `5` (exact)
    /// - `1-5` (range)
    /// - `*/15` (step)
    /// - `1,3,5` (list)
    pub fn parse(expr: &str) -> RszeroResult<Self> {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(RszeroError::Config { message: format!(
                "cron expression must have 5 fields, got {}",
                fields.len()
            ), source: None });
        }

        Ok(Self {
            minutes: parse_field(fields[0], 0, 59)?,
            hours: parse_field(fields[1], 0, 23)?,
            days: parse_field(fields[2], 1, 31)?,
            months: parse_field(fields[3], 1, 12)?,
            weekdays: parse_field(fields[4], 0, 6)?,
        })
    }

    /// Calculate the next execution time after `from`.
    pub fn next_after(&self, from: chrono::NaiveDateTime) -> Option<chrono::NaiveDateTime> {
        let mut candidate = from + chrono::Duration::minutes(1);
        candidate = candidate.with_second(0).unwrap_or(candidate).with_nanosecond(0).unwrap_or(candidate);

        // Search up to 4 years ahead
        for _ in 0..(4 * 365 * 24 * 60) {
            if self.matches(candidate) {
                return Some(candidate);
            }
            candidate += chrono::Duration::minutes(1);
        }
        None
    }

    /// Check if a datetime matches this cron expression.
    fn matches(&self, dt: chrono::NaiveDateTime) -> bool {
        self.minutes.contains(&(dt.minute() as u8))
            && self.hours.contains(&(dt.hour() as u8))
            && self.days.contains(&(dt.day() as u8))
            && self.months.contains(&(dt.month() as u8))
            && self.weekdays.contains(&(dt.weekday().num_days_from_sunday() as u8))
    }
}

fn parse_field(field: &str, min: u8, max: u8) -> RszeroResult<Vec<u8>> {
    if field == "*" {
        return Ok((min..=max).collect());
    }

    // Handle step: */15
    if let Some(rest) = field.strip_prefix("*/") {
        let step: u8 = rest.parse()
            .map_err(|e| RszeroError::Config { message: format!("invalid cron step: {}: {}", field, e), source: None })?;
        if step == 0 {
            return Err(RszeroError::Config { message: "cron step cannot be zero".into(), source: None });
        }
        return Ok((min..=max).step_by(step as usize).collect());
    }

    // Handle list: 1,3,5
    if field.contains(',') {
        let mut values = Vec::new();
        for part in field.split(',') {
            values.extend(parse_part(part, min, max)?);
        }
        values.sort_unstable();
        values.dedup();
        return Ok(values);
    }

    // Handle single value or range
    parse_part(field, min, max)
}

fn parse_part(part: &str, min: u8, max: u8) -> RszeroResult<Vec<u8>> {
    if part.contains('-') {
        let mut iter = part.split('-');
        let start: u8 = iter.next().unwrap_or("").trim().parse()
            .map_err(|e| RszeroError::Config { message: format!("invalid cron range start: {}: {}", part, e), source: None })?;
        let end: u8 = iter.next().unwrap_or("").trim().parse()
            .map_err(|e| RszeroError::Config { message: format!("invalid cron range end: {}: {}", part, e), source: None })?;
        if start > end || start < min || end > max {
            return Err(RszeroError::Config { message: format!(
                "cron range {}-{} out of bounds [{}, {}]",
                start, end, min, max
            ), source: None });
        }
        Ok((start..=end).collect())
    } else {
        let val: u8 = part.parse()
            .map_err(|e| RszeroError::Config { message: format!("invalid cron value: {}: {}", part, e), source: None })?;
        if val < min || val > max {
            return Err(RszeroError::Config { message: format!(
                "cron value {} out of bounds [{}, {}]",
                val, min, max
            ), source: None });
        }
        Ok(vec![val])
    }
}

/// Cron-aware scheduler that wraps the basic [`Scheduler`].
pub struct CronScheduler;

impl CronScheduler {
    /// Create a new cron scheduler.
    pub fn new() -> Self {
        Self
    }

    /// Schedule a task using a cron expression.
    ///
    /// The task runs at every matching time. Uses a sleep loop
    /// with per-minute resolution.
    pub async fn cron<F, Fut>(self, expr: CronExpr, task: F) -> JobHandle
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = async {
                        let now = chrono::Local::now().naive_local();
                        if let Some(next) = expr.next_after(now) {
                            let delay = next.signed_duration_since(now);
                            if delay.num_milliseconds() > 0 {
                                tokio::time::sleep(Duration::from_millis(delay.num_milliseconds() as u64)).await;
                            }
                            task().await;
                            // Small sleep to avoid double-firing within the same minute
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        } else {
                            tokio::time::sleep(Duration::from_secs(60)).await;
                        }
                    } => {}
                    _ = cancel_clone.cancelled() => break,
                }
            }
        });
        JobHandle { _handle: handle, cancel }
    }
}

impl Default for CronScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_parse_star() {
        let cron = CronExpr::parse("* * * * *").unwrap();
        assert_eq!(cron.minutes.len(), 60);
        assert_eq!(cron.hours.len(), 24);
    }

    #[test]
    fn test_cron_parse_exact() {
        let cron = CronExpr::parse("30 14 * * *").unwrap();
        assert_eq!(cron.minutes, vec![30]);
        assert_eq!(cron.hours, vec![14]);
    }

    #[test]
    fn test_cron_parse_range() {
        let cron = CronExpr::parse("0 9-17 * * 1-5").unwrap();
        assert_eq!(cron.hours, (9..=17).collect::<Vec<_>>());
        assert_eq!(cron.weekdays, (1..=5).collect::<Vec<_>>());
    }

    #[test]
    fn test_cron_parse_step() {
        let cron = CronExpr::parse("*/15 * * * *").unwrap();
        assert_eq!(cron.minutes, vec![0, 15, 30, 45]);
    }

    #[test]
    fn test_cron_parse_list() {
        let cron = CronExpr::parse("0,30 * * * *").unwrap();
        assert_eq!(cron.minutes, vec![0, 30]);
    }

    #[test]
    fn test_cron_parse_invalid_fields() {
        assert!(CronExpr::parse("* * * *").is_err());
        assert!(CronExpr::parse("* * * * * *").is_err());
    }

    #[test]
    fn test_cron_next_after() {
        let cron = CronExpr::parse("0 12 * * *").unwrap(); // noon every day
        let from = chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let next = cron.next_after(from).unwrap();
        assert_eq!(next.hour(), 12);
        assert_eq!(next.minute(), 0);
    }

    #[test]
    fn test_cron_matches() {
        let cron = CronExpr::parse("30 14 15 3 *").unwrap();
        let dt = chrono::NaiveDate::from_ymd_opt(2024, 3, 15)
            .unwrap()
            .and_hms_opt(14, 30, 0)
            .unwrap();
        assert!(cron.matches(dt));
    }
}
