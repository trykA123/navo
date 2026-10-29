use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use super::Pool;

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn a_warmed_helper_is_handed_out_once() {
    let pool = Pool::default();
    pool.warm(|| Ok::<_, ()>(7)).unwrap();
    assert_eq!(pool.take(WAIT), Some(7));
    assert_eq!(pool.take(WAIT), None);
}

#[test]
fn warming_runs_only_once() {
    let pool = Pool::default();
    let loads = AtomicUsize::new(0);
    for _ in 0..3 {
        pool.warm(|| {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(1)
        })
        .unwrap();
    }
    assert_eq!(loads.load(Ordering::SeqCst), 1);
}

#[test]
fn a_failed_warm_is_not_retried() {
    let pool = Pool::<u8>::default();
    assert_eq!(pool.warm(|| Err("no helper")), Err("no helper"));
    assert_eq!(pool.warm(|| Ok::<_, &str>(1)), Ok(()));
    assert_eq!(pool.take(WAIT), None);
}

#[test]
fn taking_before_any_warm_stops_a_later_warm() {
    let pool = Pool::<u8>::default();
    assert_eq!(pool.take(WAIT), None);
    pool.warm(|| Ok::<_, ()>(1)).unwrap();
    assert_eq!(pool.take(WAIT), None);
}

#[test]
fn a_take_during_warming_waits_for_the_helper() {
    let pool = Arc::new(Pool::default());
    let (loading, loaded) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let warm = {
        let pool = pool.clone();
        thread::spawn(move || {
            pool.warm(|| {
                loading.send(()).unwrap();
                released.recv().unwrap();
                Ok::<_, ()>(5)
            })
        })
    };
    loaded.recv_timeout(WAIT).unwrap();
    let taker = {
        let pool = pool.clone();
        thread::spawn(move || pool.take(WAIT))
    };
    thread::sleep(Duration::from_millis(50));
    release.send(()).unwrap();
    warm.join().unwrap().unwrap();
    assert_eq!(taker.join().unwrap(), Some(5));
}

#[test]
fn a_take_gives_up_when_warming_takes_too_long() {
    let pool = Arc::new(Pool::<u8>::default());
    let (loading, loaded) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let warm = {
        let pool = pool.clone();
        thread::spawn(move || {
            pool.warm(|| {
                loading.send(()).unwrap();
                released.recv().unwrap();
                Ok::<_, ()>(5)
            })
        })
    };
    loaded.recv_timeout(WAIT).unwrap();
    assert_eq!(pool.take(Duration::from_millis(50)), None);
    release.send(()).unwrap();
    warm.join().unwrap().unwrap();
    assert_eq!(pool.take(WAIT), Some(5));
}

#[test]
fn a_returned_helper_is_kept_and_a_second_one_is_dropped() {
    let pool = Pool::default();
    let first = Arc::new(());
    let second = Arc::new(());
    pool.put(first.clone());
    pool.put(second.clone());
    assert_eq!(Arc::strong_count(&first), 2);
    assert_eq!(Arc::strong_count(&second), 1);
}

#[test]
fn closing_drops_the_idle_helper_and_refuses_later_ones() {
    let pool = Pool::default();
    let kept = Arc::new(());
    pool.put(kept.clone());
    pool.close();
    assert_eq!(Arc::strong_count(&kept), 1);
    let late = Arc::new(());
    pool.put(late.clone());
    assert_eq!(Arc::strong_count(&late), 1);
    let loads = AtomicUsize::new(0);
    pool.warm(|| {
        loads.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(Arc::new(()))
    })
    .unwrap();
    assert_eq!(loads.load(Ordering::SeqCst), 0);
    assert!(pool.take(WAIT).is_none());
}

#[test]
fn a_helper_that_finishes_warming_after_close_is_dropped() {
    let pool = Pool::default();
    let helper = Arc::new(());
    let seen = helper.clone();
    pool.warm(|| {
        pool.close();
        Ok::<_, ()>(seen)
    })
    .unwrap();
    assert_eq!(Arc::strong_count(&helper), 1);
}

#[test]
fn a_warm_helper_finishing_after_a_returned_one_is_the_one_dropped() {
    let pool = Pool::default();
    let cold = Arc::new(());
    let warm = Arc::new(());
    let loaded = warm.clone();
    pool.warm(|| {
        pool.put(cold.clone());
        Ok::<_, ()>(loaded)
    })
    .unwrap();
    assert_eq!(Arc::strong_count(&warm), 1);
    assert_eq!(Arc::strong_count(&cold), 2);
    assert!(Arc::ptr_eq(&pool.take(WAIT).unwrap(), &cold));
}

#[test]
fn settling_waits_for_a_warm_up_in_progress_up_to_the_bound() {
    let pool = Arc::new(Pool::<u8>::default());
    let (loading, loaded) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let warm = {
        let pool = pool.clone();
        thread::spawn(move || {
            pool.warm(|| {
                loading.send(()).unwrap();
                released.recv().unwrap();
                Ok::<_, ()>(5)
            })
        })
    };
    loaded.recv_timeout(WAIT).unwrap();
    let started = std::time::Instant::now();
    pool.settle(Duration::from_millis(100));
    assert!(started.elapsed() >= Duration::from_millis(100));
    release.send(()).unwrap();
    warm.join().unwrap().unwrap();
    let started = std::time::Instant::now();
    pool.settle(WAIT);
    assert!(started.elapsed() < Duration::from_millis(100));
}
