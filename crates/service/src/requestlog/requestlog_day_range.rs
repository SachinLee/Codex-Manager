use chrono::{Duration, Local, LocalResult, TimeZone};

pub(crate) const MAX_REQUESTED_DAY_RANGE_SECS: i64 = 48 * 60 * 60;

fn local_day_bounds_ts() -> Result<(i64, i64), String> {
    let now = Local::now();
    let today = now.date_naive();
    let start_naive = today
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "build local start-of-day failed".to_string())?;
    let tomorrow_naive = (today + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "build local end-of-day failed".to_string())?;

    let start = match Local.from_local_datetime(&start_naive) {
        LocalResult::Single(value) => value.timestamp(),
        LocalResult::Ambiguous(a, b) => a.timestamp().min(b.timestamp()),
        LocalResult::None => now.timestamp(),
    };
    let end = match Local.from_local_datetime(&tomorrow_naive) {
        LocalResult::Single(value) => value.timestamp(),
        LocalResult::Ambiguous(a, b) => a.timestamp().max(b.timestamp()),
        LocalResult::None => start + 24 * 60 * 60,
    };
    Ok((start, end.max(start)))
}

pub(crate) fn resolve_day_bounds_ts(
    day_start_ts: Option<i64>,
    day_end_ts: Option<i64>,
) -> Result<(i64, i64), String> {
    match (day_start_ts, day_end_ts) {
        (Some(start), Some(end)) => {
            if end <= start {
                return Err("dayEndTs must be greater than dayStartTs".to_string());
            }
            if end - start > MAX_REQUESTED_DAY_RANGE_SECS {
                return Err("requested day range is too large".to_string());
            }
            Ok((start, end))
        }
        (None, None) => local_day_bounds_ts(),
        _ => Err("dayStartTs and dayEndTs must be provided together".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_day_bounds_ts, MAX_REQUESTED_DAY_RANGE_SECS};

    #[test]
    fn resolve_day_bounds_uses_requested_range_when_complete() {
        assert_eq!(
            resolve_day_bounds_ts(Some(1_700_000_000), Some(1_700_086_400)).unwrap(),
            (1_700_000_000, 1_700_086_400)
        );
    }

    #[test]
    fn resolve_day_bounds_rejects_partial_range() {
        let error = resolve_day_bounds_ts(Some(1_700_000_000), None).unwrap_err();
        assert!(error.contains("provided together"));
    }

    #[test]
    fn resolve_day_bounds_rejects_oversized_range() {
        let error =
            resolve_day_bounds_ts(Some(0), Some(MAX_REQUESTED_DAY_RANGE_SECS + 1)).unwrap_err();
        assert!(error.contains("too large"));
    }
}
