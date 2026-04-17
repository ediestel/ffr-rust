use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ffr_core::classify;
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn make_text(path: &std::path::Path, line_count: usize, line_len: usize) {
    let mut f = fs::File::create(path).unwrap();
    let line = "hello world ".repeat(line_len / 12 + 1);
    for _ in 0..line_count {
        writeln!(f, "{}", &line[..line_len]).unwrap();
    }
}

fn make_binary(path: &std::path::Path, size: usize) {
    let mut data = vec![0u8; size];
    for (i, b) in data.iter_mut().enumerate() {
        *b = (i % 256) as u8;
    }
    fs::write(path, data).unwrap();
}

fn bench_classify(c: &mut Criterion) {
    let tmp = TempDir::new().unwrap();
    let mut group = c.benchmark_group("classify_path");

    let text_small = tmp.path().join("small.txt");
    make_text(&text_small, 100, 80);

    let text_large = tmp.path().join("large.rs");
    make_text(&text_large, 50_000, 100);

    let bin = tmp.path().join("binary.bin");
    make_binary(&bin, 64 * 1024);

    for (label, path) in [
        ("text_small_8KB", text_small.to_string_lossy().into_owned()),
        ("text_large_5MB", text_large.to_string_lossy().into_owned()),
        ("binary_64KB", bin.to_string_lossy().into_owned()),
    ] {
        group.bench_with_input(BenchmarkId::from_parameter(label), &path, |b, p| {
            b.iter(|| classify::classify_path(p, 4096, 2 * 1024 * 1024, 1000).unwrap());
        });
    }
    group.finish();
}

criterion_group!(benches, bench_classify);
criterion_main!(benches);
