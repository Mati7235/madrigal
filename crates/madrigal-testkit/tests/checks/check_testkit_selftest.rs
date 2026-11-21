use madrigal_testkit as kit;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

fn move_row(model: &VecModel<slint::SharedString>, from: usize, to: usize) {
    if from == to || from >= model.row_count() || to >= model.row_count() {
        return;
    }
    let item = model.row_data(from).unwrap();
    model.remove(from);
    model.insert(to, item);
}

#[test]
fn testkit_selftest() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", tmp.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", tmp.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));

    kit::init_headless();
    let demo = kit::Demo::new().unwrap();
    assert_eq!(demo.get_count(), 0);

    let songs = Rc::new(VecModel::from(vec![
        "a".into(),
        "b".into(),
        "c".into(),
        "d".into(),
    ]));
    demo.set_songs(ModelRc::from(songs.clone()));

    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(100),
        {
            let demo = demo.as_weak();
            move || demo.upgrade().unwrap().invoke_tick()
        },
    );

    let from = Rc::new(Cell::new(-1i32));
    let press_y = Rc::new(Cell::new(0.0f32));
    let row_h = 36.0f32;
    demo.on_drag_start({
        let demo = demo.as_weak();
        let from = from.clone();
        let press_y = press_y.clone();
        move |idx, y| {
            from.set(idx);
            press_y.set(y);
            demo.upgrade().unwrap().set_drag_from(idx);
            demo.upgrade().unwrap().set_drag_to(idx);
        }
    });
    demo.on_drag_move({
        let demo = demo.as_weak();
        let from = from.clone();
        let press_y = press_y.clone();
        move |y| {
            let current = from.get();
            if current >= 0 {
                let mut target = current + ((y - press_y.get()) / row_h).round() as i32;
                target = target.clamp(0, 3);
                demo.upgrade().unwrap().set_drag_to(target);
            }
        }
    });
    let rows = songs.clone();
    demo.on_drag_end({
        let demo = demo.as_weak();
        let from = from.clone();
        move || {
            let view = demo.upgrade().unwrap();
            let (start, end) = (view.get_drag_from(), view.get_drag_to());
            if start >= 0 && end >= 0 && start != end {
                move_row(&rows, start as usize, end as usize);
            }
            view.set_drag_from(-1);
            view.set_drag_to(-1);
            from.set(-1);
        }
    });

    kit::click_by_id(&demo, "Demo::inc-btn").unwrap();
    assert_eq!(demo.get_count(), 1);

    let missing = kit::click_by_id(&demo, "Demo::no-existe");
    assert!(matches!(missing, Err(kit::Error::ElementNotFound(_))));

    kit::mock_advance(Duration::from_millis(1000));
    assert_eq!(demo.get_ticks(), 10);

    let handles: Vec<_> = kit::ElementHandle::find_by_element_id(&demo, "Demo::ta").collect();
    assert_eq!(handles.len(), 4);
    let pos = handles[0].absolute_position();
    let size = handles[0].size();
    let target = slint::LogicalPosition::new(pos.x + size.width / 2.0, pos.y + 2.0 * row_h);
    kit::click(&handles[0]);
    assert_eq!(demo.get_count(), 1);
    handles[0].mock_drag(target, slint::platform::PointerEventButton::Left);
    let order: Vec<String> =
        (0..songs.row_count()).map(|i| songs.row_data(i).unwrap().to_string()).collect();
    assert_eq!(order, vec!["b", "c", "a", "d"]);

    demo.show().unwrap();
    let shot = tmp.path().join("demo.png");
    kit::snapshot_to(demo.window(), &shot).unwrap();
    let bytes = std::fs::read(&shot).unwrap();
    assert!(bytes.len() > 100);
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);

    let overlap = kit::LintOverlap::new().unwrap();
    overlap.show().unwrap();
    let violations = kit::lint_layout(&overlap, (240.0, 120.0));
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert_eq!(
        violations[0].kind,
        kit::ViolationKind::Overlap("LintOverlap::first-text".to_string())
    );
    assert_eq!(violations[0].id, "LintOverlap::second-text");

    let tight = kit::lint_layout(&overlap, (100.0, 100.0));
    assert!(
        tight.iter().any(|violation| violation.kind == kit::ViolationKind::OutOfWindow),
        "{tight:?}"
    );

    let nested = kit::LintNested::new().unwrap();
    nested.show().unwrap();
    let violations = kit::lint_layout(&nested, (240.0, 120.0));
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert_eq!(
        violations[0].kind,
        kit::ViolationKind::Overlap("LintNested::nested-a".to_string())
    );
    assert_eq!(violations[0].id, "LintNested::nested-b");

    let clean = kit::LintClean::new().unwrap();
    clean.show().unwrap();
    let violations = kit::lint_layout(&clean, (240.0, 120.0));
    assert!(violations.is_empty(), "{violations:?}");
}
