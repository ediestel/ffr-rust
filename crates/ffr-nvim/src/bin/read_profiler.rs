//! Simple profiling harness for read + classify operations.
//!
//! Usage:
//!   cargo run --release -p ffr-nvim --bin read_profiler -- <path> [iterations]

use std::env;
use std::time::Instant;

use ffr_core::{classify, lines, read};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: read_profiler <path> [iterations]");
        std::process::exit(1);
    }
    let path = &args[1];
    let iterations: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);

    // Classify
    let t0 = Instant::now();
    for _ in 0..iterations {
        let _ = classify::classify_path(path, 4096, 2 * 1024 * 1024, 1000).unwrap();
    }
    let classify_ns = t0.elapsed().as_nanos() / iterations as u128;
    println!("classify_path: {classify_ns} ns/iter");

    // Line index build
    let t0 = Instant::now();
    let _ = lines::build_line_index(path).unwrap();
    let build_us = t0.elapsed().as_micros();
    println!("build_line_index (once): {build_us} us");

    // Read chunk 0
    let t0 = Instant::now();
    for _ in 0..iterations {
        let _ = read::read_chunk(path, 0, 64 * 1024).unwrap();
    }
    let chunk_ns = t0.elapsed().as_nanos() / iterations as u128;
    println!("read_chunk(0, 64KB): {chunk_ns} ns/iter");

    // Read lines 100..200
    let t0 = Instant::now();
    for _ in 0..iterations {
        let _ = lines::read_lines(path, 1, 200).unwrap();
    }
    let lines_ns = t0.elapsed().as_nanos() / iterations as u128;
    println!("read_lines(1, 200): {lines_ns} ns/iter");
}
