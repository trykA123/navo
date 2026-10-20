#[path = "../../../tests/blf_build/mod.rs"]
mod build;

use super::window::BinaryPlan;
use build::*;

fn trace(bytes: &[u8]) -> crate::Trace {
    crate::index_blf(
        std::io::Cursor::new(bytes),
        crate::Watch {
            cancel: &std::sync::atomic::AtomicBool::new(false),
            progress: &mut |_| {},
        },
    )
    .unwrap()
}

#[test]
fn window_inflates_only_needed_containers_and_reuses_the_lru() {
    let mut containers = Vec::new();
    for _ in 0..10 {
        let mut inner = Vec::new();
        for at in 0..128 {
            inner.extend(object(1, 1, at, &classic(1, 0, 16)));
        }
        containers.push(container(&inner, true));
    }
    let bytes = file(&containers, 1280);
    let trace = trace(&bytes);
    let plan = BinaryPlan::new(&trace, &[640, 672, 704, 736]);
    let (decoded, inflated) = plan.decode_counted(&bytes);
    assert_eq!(decoded.len(), 4);
    assert_eq!(inflated, 1);
    let (_, inflated) = BinaryPlan::new(&trace, &[0, 640, 1279]).decode_counted(&bytes);
    assert_eq!(inflated, 3);
}

#[test]
fn split_object_checkpoint_starts_in_its_first_container() {
    let mut first = Vec::new();
    for at in 0..32 {
        first.extend(object(1, 1, at, &classic(1, 0, 16)));
    }
    let split = object(101, 1, 40, &fd(101, 7 << 12));
    let offset = first.len();
    first.extend(&split[..2]);
    let bytes = file(&[container(&first, true), container(&split[2..], true)], 33);
    let trace = trace(&bytes);
    assert_eq!(trace.binary_blocks[1].position.container, 144);
    assert_eq!(trace.binary_blocks[1].position.inner, offset as u32);
    let (decoded, inflated) = BinaryPlan::new(&trace, &[32]).decode_counted(&bytes);
    assert_eq!(inflated, 2);
    let crate::Body::Frame(frame) = &decoded[0].body else {
        unreachable!()
    };
    assert_eq!(frame.data.len(), 64);
}
