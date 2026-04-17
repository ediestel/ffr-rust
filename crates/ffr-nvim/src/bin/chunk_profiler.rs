//! Sequential chunk traversal profiler.
//!
//! Usage:
//!   cargo run --release -p ffr-nvim --bin chunk_profiler -- <path> [chunk_bytes]

use std::env;
use std::time::Instant;

use ffr_core::read;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: chunk_profiler <path> [chunk_bytes]");
        std::process::exit(1);
    }
    let path = &args[1];
    let chunk_bytes: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(64 * 1024);

    let t0 = Instant::now();
    let mut chunk_id = 0u64;
    let mut total_bytes = 0u64;
    loop {
        let r = read::read_chunk(path, chunk_id, chunk_bytes).unwrap();
        total_bytes += (r.byte_end - r.byte_start) as u64;
        if r.eof {
            break;
        }
        chunk_id += 1;
    }
    let elapsed = t0.elapsed();
    let mb = total_bytes as f64 / (1024.0 * 1024.0);
    let mbps = mb / elapsed.as_secs_f64();
    println!(
        "{} chunks, {:.2} MB total, {:?} elapsed, {:.1} MB/s",
        chunk_id + 1,
        mb,
        elapsed,
        mbps
    );
}
