//! Unit tests for `Shared`: phase and slow-entry telemetry, and the reading heartbeat.

use std::sync::mpsc;

use navo_core::ProviderPath;

use super::*;
use crate::session::ComparePhase;

#[test]
fn shared_records_and_drains_phase_and_slow_entries_at_threshold() {
    let (tx, rx) = mpsc::channel();
    let gen1 = Generation::new(1);
    let shared = Shared::new(tx, gen1);
    let phase = PhaseTelemetry {
        phase: ComparePhase::QuickCheck,
        done: 4,
        total: Some(10),
    };
    let slow_a = RelPath::new(ProviderPath::from_text("slow.bin"));
    let slow_b = RelPath::new(ProviderPath::from_text("other.bin"));

    assert!(shared.record_phase(gen1, phase));
    assert!(!shared.record_slow(gen1, slow_a.clone(), 249));
    assert!(shared.record_slow(gen1, slow_a.clone(), 250));
    shared.set_slow_threshold_ms(10);
    assert!(!shared.record_slow(gen1, slow_b.clone(), 9));
    assert!(shared.record_slow(gen1, slow_b.clone(), 10));

    assert!(shared.report_problem(gen1, Reason::ReadError));
    let batch = rx.try_recv().expect("batch emitted");
    assert_eq!(batch.phase, Some(phase));
    assert_eq!(batch.slow.len(), 2);
    assert_eq!(
        (&batch.slow[0].rel, batch.slow[0].elapsed_ms),
        (&slow_a, 250)
    );
    assert_eq!(
        (&batch.slow[1].rel, batch.slow[1].elapsed_ms),
        (&slow_b, 10)
    );

    assert!(!shared.record_phase(Generation::new(99), phase));
    assert!(!shared.record_slow(Generation::new(99), slow_b, 100));
    assert!(shared.finish(gen1));
    let final_batch = rx.try_recv().expect("final batch emitted");
    assert_eq!((final_batch.phase, final_batch.slow.len()), (None, 0));
}
