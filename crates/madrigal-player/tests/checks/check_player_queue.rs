use madrigal_core::{TrackIdentity, TrackSource};
use madrigal_player::queue::Queue;
use madrigal_settings::RepeatMode;

fn track(name: &str) -> TrackIdentity {
    TrackIdentity::new(TrackSource::Local, name)
}

fn names(queue: &Queue) -> Vec<String> {
    queue.items().iter().map(|t| t.id.clone()).collect()
}

#[test]
fn queue_repeat_cycles_shuffle_moves_and_lock() {
    let (a, b, c) = (track("a"), track("b"), track("c"));
    let mut queue = Queue::new();
    assert!(queue.is_empty());
    assert_eq!(queue.current(), None);
    assert_eq!(queue.advance(), None);
    assert_eq!(queue.back(), None);
    assert_eq!(queue.peek_next(), None);

    assert!(queue.replace(vec![a.clone(), b.clone(), c.clone()], 0));
    assert_eq!(queue.current(), Some(&a));
    assert_eq!(queue.current_index(), Some(0));

    assert_eq!(queue.advance(), Some(&b));
    assert_eq!(queue.advance(), Some(&c));
    assert_eq!(queue.peek_next(), None);
    assert_eq!(queue.advance(), None);
    assert_eq!(queue.advance(), None);
    assert_eq!(queue.current(), Some(&c));
    assert_eq!(queue.current_index(), Some(2));
    assert_eq!(queue.back(), Some(&b));
    assert_eq!(queue.back(), Some(&a));
    assert_eq!(queue.back(), Some(&a));
    assert_eq!(queue.play_at(1), Some(&b));
    assert_eq!(queue.play_at(9), None);
    assert_eq!(queue.current(), Some(&b));

    queue.set_repeat(RepeatMode::All);
    queue.play_at(2);
    assert_eq!(queue.advance(), Some(&a));
    assert_eq!(queue.peek_next(), Some(&b));
    queue.play_at(0);
    assert_eq!(queue.back(), Some(&a));

    queue.set_repeat(RepeatMode::One);
    queue.play_at(1);
    assert_eq!(queue.advance(), Some(&b));
    assert_eq!(queue.advance(), Some(&b));
    queue.set_repeat(RepeatMode::Off);

    let batch: Vec<TrackIdentity> = (0..6).map(|i| track(&format!("t{i}"))).collect();
    let mut first = Queue::new();
    assert!(first.replace(batch.clone(), 2));
    first.reshuffle(7);
    let mut second = Queue::new();
    assert!(second.replace(batch.clone(), 2));
    second.reshuffle(7);
    assert_eq!(first.order_indices(), second.order_indices());
    let mut sorted = first.order_indices().to_vec();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![0, 1, 2, 3, 4, 5]);
    assert_eq!(first.current(), Some(&batch[2]));

    let mut seen_first = [false; 6];
    let mut distinct = std::collections::HashSet::new();
    let mut cycler = Queue::new();
    cycler.set_shuffle(true);
    for _ in 0..200 {
        assert!(cycler.replace(batch.clone(), 0));
        distinct.insert(cycler.order_indices().to_vec());
        if let Some(&head) = cycler.order_indices().first() {
            seen_first[head] = true;
        }
    }
    assert!(seen_first.iter().all(|&s| s), "every track leads: {seen_first:?}");
    assert!(distinct.len() > 100, "seed spread: {}", distinct.len());

    let mut permanent = Queue::new();
    permanent.set_permanent_shuffle(true);
    assert!(permanent.replace(batch.clone(), 0));
    assert!(permanent.is_shuffled());
    let mut sorted = permanent.order_indices().to_vec();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![0, 1, 2, 3, 4, 5]);
    let order_one = permanent.order_indices().to_vec();
    let mut twin = Queue::new();
    twin.set_permanent_shuffle(true);
    assert!(twin.replace(batch.clone(), 0));
    assert_eq!(twin.order_indices(), order_one.as_slice());
    assert!(permanent.replace(batch.clone(), 3));
    assert!(permanent.is_shuffled());
    assert_eq!(permanent.current_index(), Some(3));
    permanent.set_shuffle(false);
    assert!(permanent.is_shuffled());
    permanent.set_permanent_shuffle(false);
    assert!(!permanent.is_shuffled());
    assert_eq!(permanent.current(), Some(&batch[3]));
    let identity: Vec<usize> = (0..6).collect();
    assert_eq!(permanent.order_indices(), identity.as_slice());

    let mut edit = Queue::new();
    edit.replace(
        vec![track("a"), track("b"), track("c"), track("d")],
        1,
    );
    assert!(edit.remove(0));
    assert_eq!(names(&edit), vec!["b", "c", "d"]);
    assert_eq!(edit.current(), Some(&track("b")));
    assert_eq!(edit.current_index(), Some(0));
    assert!(edit.move_item(2, 0));
    assert_eq!(names(&edit), vec!["d", "b", "c"]);
    assert_eq!(edit.current(), Some(&track("b")));
    assert_eq!(edit.current_index(), Some(1));
    assert!(edit.remove(1));
    assert_eq!(edit.current(), Some(&track("c")));
    assert!(!edit.remove(9));
    assert!(edit.clear());
    assert!(edit.is_empty());

    let mut shuffled = Queue::new();
    shuffled.replace(batch.clone(), 4);
    shuffled.set_shuffle(true);
    let playing = shuffled.current().cloned().unwrap();
    let victim = (0..6).find(|&i| batch[i] != playing).unwrap();
    assert!(shuffled.remove(victim));
    assert_eq!(shuffled.current(), Some(&playing));

    let mut locked = Queue::new();
    locked.replace(vec![a.clone(), b.clone()], 0);
    locked.set_locked(true);
    assert!(locked.locked());
    assert!(!locked.replace(vec![c.clone()], 0));
    assert!(!locked.push(c.clone()));
    assert!(!locked.insert(0, c.clone()));
    assert!(!locked.remove(0));
    assert!(!locked.move_item(0, 1));
    assert!(!locked.clear());
    assert_eq!(names(&locked), vec!["a", "b"]);
    assert_eq!(locked.advance(), Some(&b));
    assert_eq!(locked.back(), Some(&a));
    locked.set_locked(false);
    assert!(locked.push(c.clone()));
    assert_eq!(names(&locked), vec!["a", "b", "c"]);
}
