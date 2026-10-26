//! Canonical completed-record ordering and calendar presentation. Host refreshes
//! also sample published-path availability through the file adapter; no engine
//! callbacks, destination decisions, or inferred completion belong here.
use crate::downloads_view::{CompletedDownload as Row, DownloadsGroup, DownloadsList};
use browser_core::downloads::{CompletedDownload, CompletedDownloadId, DownloadTime};
use chrono::{DateTime, Local, Utc};

const WINDOWS_TO_UNIX_MICROS: i64 = 11_644_473_600_000_000;

fn instant(time: DownloadTime) -> Option<DateTime<Utc>> {
    match time {
        DownloadTime::WindowsEpochMicros(raw) => {
            DateTime::from_timestamp_micros(raw.checked_sub(WINDOWS_TO_UNIX_MICROS)?)
        }
        DownloadTime::Unknown | DownloadTime::Invalid(_) => None,
    }
}

pub(crate) fn completed(
    records: Vec<CompletedDownload>,
    filtered: bool,
) -> DownloadsList<CompletedDownloadId> {
    let now = Utc::now();
    // Native references show a file kind plus an optional source host; date
    // belongs to the calendar heading rather than each completed subtitle.
    present(
        records,
        filtered,
        |date| date_heading(date, now),
        crate::download_files::published_subtitle,
    )
}

fn present(
    mut records: Vec<CompletedDownload>,
    filtered: bool,
    mut heading: impl FnMut(DateTime<Utc>) -> String,
    mut subtitle: impl FnMut(&CompletedDownload) -> String,
) -> DownloadsList<CompletedDownloadId> {
    if filtered && records.is_empty() {
        return DownloadsList::FilteredEmpty;
    }
    // Stable sorting retains canonical order for equal timestamps. Unknown and
    // unrepresentable times stay visible after dated records; never use "now".
    records.sort_by_cached_key(|record| std::cmp::Reverse(instant(record.finished_at())));
    let mut groups: Vec<DownloadsGroup<CompletedDownloadId>> = Vec::new();
    for record in records {
        let label = instant(record.finished_at())
            .map(&mut heading)
            .unwrap_or_default();
        let row = Row {
            token: record.id(),
            profile: record.id().profile(),
            title: record.final_name().to_string_lossy().into_owned(),
            subtitle: subtitle(&record),
        };
        // The reference grouping candidate keys by the rendered heading. In
        // particular, relative weeks/months are not fixed-duration buckets.
        if let Some(group) = groups.iter_mut().find(|group| group.heading == label) {
            group.rows.push(row);
        } else {
            groups.push(DownloadsGroup {
                heading: label,
                rows: vec![row],
            });
        }
    }
    DownloadsList::Completed(groups)
}

#[cfg(not(target_os = "macos"))]
fn date_heading(date: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let day = date.with_timezone(&Local).date_naive();
    let today = now.with_timezone(&Local).date_naive();
    if day == today {
        "Today".into()
    } else if Some(day) == today.pred_opt() {
        "Yesterday".into()
    }
    // Platform-relative wording is not established outside macOS. Retain the
    // actual calendar date instead of assuming 7/30-day localized thresholds.
    else {
        day.format("%b %-d, %Y").to_string()
    }
}

#[cfg(target_os = "macos")]
fn date_heading(date: DateTime<Utc>, now: DateTime<Utc>) -> String {
    native::heading(date, now)
        .unwrap_or_else(|| date.with_timezone(&Local).format("%b %-d, %Y").to_string())
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use objc2::{
        class, msg_send,
        rc::Retained,
        runtime::{AnyObject, Bool},
    };
    use objc2_foundation::NSString;

    pub(super) fn heading(date: DateTime<Utc>, now: DateTime<Utc>) -> Option<String> {
        // Foundation owns calendar/time-zone/locale rules. A fresh formatter
        // picks up changed system settings; no hardcoded month boundary.
        objc2::rc::autoreleasepool(|_| unsafe {
            let date: Retained<AnyObject> = Retained::retain(
                msg_send![class!(NSDate), dateWithTimeIntervalSince1970: date.timestamp_micros() as f64 / 1_000_000.],
            )?;
            let now: Retained<AnyObject> = Retained::retain(
                msg_send![class!(NSDate), dateWithTimeIntervalSince1970: now.timestamp_micros() as f64 / 1_000_000.],
            )?;
            let calendar: Retained<AnyObject> =
                Retained::retain(msg_send![class!(NSCalendar), autoupdatingCurrentCalendar])?;
            let today: Bool = msg_send![&*calendar, isDate: &*date, inSameDayAsDate: &*now];
            if today.as_bool() {
                return Some("Today".into());
            }
            // NSCalendarUnitDay = 1 << 4. Calendar addition preserves local
            // day semantics through DST, unlike subtracting 24 elapsed hours.
            let yesterday: Retained<AnyObject> = Retained::retain(
                msg_send![&*calendar, dateByAddingUnit: 16usize, value: -1isize, toDate: &*now, options: 0usize],
            )?;
            let yesterday: Bool =
                msg_send![&*calendar, isDate: &*date, inSameDayAsDate: &*yesterday];
            if yesterday.as_bool() {
                return Some("Yesterday".into());
            }
            let allocated: *mut AnyObject = msg_send![class!(NSRelativeDateTimeFormatter), alloc];
            let formatter: Retained<AnyObject> = Retained::from_raw(msg_send![allocated, init])?;
            let _: () = msg_send![&*formatter, setUnitsStyle: 0isize];
            let _: () = msg_send![&*formatter, setDateTimeStyle: 0isize];
            let label: Retained<NSString> = Retained::retain(
                msg_send![&*formatter, localizedStringForDate: &*date, relativeToDate: &*now],
            )?;
            Some(label.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use browser_core::{downloads::DownloadCompletion, Browser};

    fn records(times: &[DownloadTime]) -> Vec<CompletedDownload> {
        let mut browser = Browser::new();
        let profile = browser.active_user_profile_id();
        for (i, time) in times.iter().enumerate() {
            browser
                .record_completed_download(
                    profile,
                    DownloadCompletion {
                        final_path: std::env::temp_dir().join(format!("record-{i}.bin")),
                        source_url: "https://fixture.test/file".into(),
                        original_url: "https://fixture.test/file".into(),
                        mime_type: "application/octet-stream".into(),
                        finished_at: *time,
                    },
                )
                .unwrap();
        }
        browser.completed_downloads(profile).unwrap().to_vec()
    }
    fn time(seconds: i64) -> DownloadTime {
        DownloadTime::WindowsEpochMicros(WINDOWS_TO_UNIX_MICROS + seconds * 1_000_000)
    }

    #[test]
    fn cef_epoch_conversion_is_checked_and_preserves_microseconds() {
        assert_eq!(instant(time(0)).unwrap().timestamp_micros(), 0);
        assert_eq!(instant(time(-1)).unwrap().timestamp_micros(), -1_000_000);
        assert_eq!(
            instant(DownloadTime::WindowsEpochMicros(
                WINDOWS_TO_UNIX_MICROS + 123
            ))
            .unwrap()
            .timestamp_micros(),
            123
        );
        for value in [
            DownloadTime::Unknown,
            DownloadTime::Invalid(i64::MAX),
            DownloadTime::WindowsEpochMicros(i64::MIN),
            DownloadTime::WindowsEpochMicros(i64::MAX),
        ] {
            assert!(instant(value).is_none());
        }
    }
    #[test]
    fn groups_by_formatted_label_newest_first_without_losing_unknown_records() {
        let records = records(&[time(1), time(3), DownloadTime::Unknown, time(2), time(3)]);
        let ids: Vec<_> = records.iter().map(|record| record.id()).collect();
        let DownloadsList::Completed(groups) = present(
            records,
            false,
            |_| "Same label".into(),
            |record| format!("subtitle-{}", record.id().sequence()),
        ) else {
            panic!()
        };
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].heading, "Same label");
        assert_eq!(
            groups[0]
                .rows
                .iter()
                .map(|row| row.token)
                .collect::<Vec<_>>(),
            [ids[1], ids[4], ids[3], ids[0]]
        );
        assert_eq!(groups[0].rows[0].title, "record-1.bin");
        assert_eq!(
            groups[0].rows[0].subtitle,
            format!("subtitle-{}", ids[1].sequence())
        );
        assert_eq!(groups[1].heading, "");
        assert_eq!(groups[1].rows[0].token, ids[2]);
    }
    #[test]
    fn empty_explicit_filter_has_distinct_presentation_without_inventing_database_copy() {
        assert!(matches!(
            present(vec![], true, |_| unreachable!(), |_| unreachable!()),
            DownloadsList::FilteredEmpty
        ));
        assert!(
            matches!(present(vec![], false, |_| unreachable!(), |_| unreachable!()), DownloadsList::Completed(groups) if groups.is_empty())
        );
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn foundation_calendar_and_relative_formatter_execute_with_real_native_objects() {
        let now = Utc::now();
        assert_eq!(native::heading(now, now).as_deref(), Some("Today"));
        // Noon avoids midnight ambiguity. Calendar-day classification is tested
        // separately from the relative formatter's locale-dependent wording.
        let local = now
            .with_timezone(&Local)
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .single()
            .unwrap();
        assert_eq!(
            native::heading(
                (local - chrono::Days::new(1)).with_timezone(&Utc),
                local.with_timezone(&Utc)
            )
            .as_deref(),
            Some("Yesterday")
        );
        assert!(!native::heading(now - chrono::Duration::days(40), now)
            .unwrap()
            .is_empty());
    }
}
