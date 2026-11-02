use std::fs::File;
use std::hint::black_box;
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use blake3::Hasher;

const BENCH_SIZE: u64 = 1024 * 1024 * 1024;
const BENCH_BLOCK: usize = 16 * 1024 * 1024;
const RUNS: usize = 3;

fn generated(file: &mut File, size: u64) {
    let mut writer = BufWriter::new(file);
    let mut block = vec![0u8; BENCH_BLOCK];
    for (index, byte) in block.iter_mut().enumerate() {
        *byte = u8::try_from(index % 251).expect("byte fits");
    }
    let mut left = size;
    while left > 0 {
        let take = usize::try_from(left.min(BENCH_BLOCK as u64)).expect("block fits");
        writer
            .write_all(&block[..take])
            .expect("bench file written");
        left -= take as u64;
    }
    writer.flush().expect("bench file flushed");
}

fn hash(path: &Path, parallel: bool) -> (Duration, [u8; 32]) {
    let mut file = File::open(path).expect("bench file opened");
    let mut buffer = vec![0u8; BENCH_BLOCK];
    let mut hasher = Hasher::new();
    let mut left = BENCH_SIZE;
    let started = Instant::now();
    while left > 0 {
        let take = usize::try_from(left.min(BENCH_BLOCK as u64)).expect("block fits");
        file.read_exact(&mut buffer[..take])
            .expect("bench file read");
        if parallel {
            hasher.update_rayon(&buffer[..take]);
        } else {
            hasher.update(&buffer[..take]);
        }
        left -= take as u64;
    }
    (started.elapsed(), *hasher.finalize().as_bytes())
}

#[test]
#[ignore = "release-only: 1 GiB single-thread against rayon hashing"]
#[allow(
    clippy::print_stderr,
    reason = "the ignored benchmark reports its measurement"
)]
fn bench_one_gib_single_against_rayon() {
    let mut file = tempfile::NamedTempFile::new().expect("bench file created");
    generated(file.as_file_mut(), BENCH_SIZE);
    let path = file.path();
    black_box(hash(path, false));
    black_box(hash(path, true));

    let mut single = Vec::with_capacity(RUNS);
    let mut parallel = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let one = hash(path, false);
        let many = hash(path, true);
        assert_eq!(one.1, many.1);
        single.push(one.0);
        parallel.push(many.0);
    }
    single.sort_unstable();
    parallel.sort_unstable();
    let single = single[RUNS / 2];
    let parallel = parallel[RUNS / 2];
    let gain = 100.0 * (1.0 - parallel.as_secs_f64() / single.as_secs_f64());
    eprintln!("1 GiB blake3: single {single:?}, rayon {parallel:?}, gain {gain:.1}%");
}
