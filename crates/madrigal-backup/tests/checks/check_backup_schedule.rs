use std::path::PathBuf;

use madrigal_backup::{
    Frequency, OverwritePolicy, ScheduleConfig, destination_for, is_due, mark_ran, next_run, poll,
};

fn config(frequency: Frequency, overwrite: OverwritePolicy, last_run: Option<i64>) -> ScheduleConfig {
    ScheduleConfig {
        frequency,
        destination_dir: PathBuf::from("/tmp/madrigal-schedule-check"),
        overwrite,
        last_run,
        redownload_missing: false,
    }
}

#[test]
fn next_run_math_for_all_frequencies() {
    let base = 1_700_000_000;
    assert_eq!(
        next_run(&config(Frequency::Daily, OverwritePolicy::Overwrite, Some(base)), base),
        base + 86_400
    );
    assert_eq!(
        next_run(&config(Frequency::Weekly, OverwritePolicy::Overwrite, Some(base)), base),
        base + 7 * 86_400
    );
    assert_eq!(
        next_run(
            &config(Frequency::IntervalSecs(3600), OverwritePolicy::Overwrite, Some(base)),
            base
        ),
        base + 3600
    );

    let jan15 = 1_705_276_800;
    let mut monthly = config(Frequency::Monthly, OverwritePolicy::Overwrite, Some(jan15));
    let feb = next_run(&monthly, jan15);
    assert_eq!(feb - jan15, 31 * 86_400);

    monthly.last_run = Some(1_707_955_200);
    let end = next_run(&monthly, 1_707_955_200);
    assert_eq!(end - 1_707_955_200, 29 * 86_400);

    let fresh = config(Frequency::Daily, OverwritePolicy::Overwrite, None);
    assert_eq!(next_run(&fresh, base), base + 86_400);
}

#[test]
fn overwrite_vs_keep_naming() {
    let base = 1_700_000_000;
    let overwrite = config(Frequency::Daily, OverwritePolicy::Overwrite, Some(base));
    assert_eq!(
        destination_for(&overwrite, base).file_name().unwrap(),
        "madrigal-backup.json"
    );

    let keep = config(Frequency::Daily, OverwritePolicy::KeepAll, Some(base));
    let first = destination_for(&keep, base);
    let second = destination_for(&keep, base + 60);
    assert_ne!(first, second);
    for path in [&first, &second] {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("madrigal-backup-"), "{name}");
        assert!(name.ends_with(".json"), "{name}");
    }
}

#[test]
fn poll_and_mark_ran_trigger_once() {
    let base = 1_700_000_000;
    let mut scheduled = config(
        Frequency::IntervalSecs(3600),
        OverwritePolicy::Overwrite,
        Some(base),
    );
    assert!(!is_due(&scheduled, base + 3599));
    assert!(poll(&scheduled, base + 3599).is_none());
    assert!(is_due(&scheduled, base + 3600));
    assert!(poll(&scheduled, base + 3600).is_some());

    mark_ran(&mut scheduled, base + 3600);
    assert_eq!(scheduled.last_run, Some(base + 3600));
    assert!(!is_due(&scheduled, base + 3600));
    assert!(is_due(&scheduled, base + 7200));
}
