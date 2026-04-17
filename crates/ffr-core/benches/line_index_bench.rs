use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use ffr_core::lines;
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn make_file(dir: &std::path::Path, name: &str, line_count: usize, line_len: usize) -> String {
    let path = dir.join(name);
    let mut f = fs::File::create(&path).unwrap();
    let line = "x".repeat(line_len);
    for _ in 0..line_count {
        writeln!(f, "{}", line).unwrap();
    }
    path.to_string_lossy().into_owned()
}

fn bench_construct_line_index(c: &mut Criterion) {
    let tmp = TempDir::new().unwrap();
    let mut group = c.benchmark_group("construct_line_index");

    for &(lines_count, line_len) in &[(1_000usize, 80usize), (10_000, 100), (100_000, 120)] {
        let path = make_file(
            tmp.path(),
            &format!("l{lines_count}.txt"),
            lines_count,
            line_len,
        );
        let bytes = lines_count * (line_len + 1);
        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines_count}_lines")),
            &path,
            |b, p| {
                b.iter(|| lines::build_line_index(p).unwrap());
            },
        );
    }
    group.finish();
}

fn bench_read_lines_range(c: &mut Criterion) {
    let tmp = TempDir::new().unwrap();
    let path = make_file(tmp.path(), "read_lines.txt", 100_000, 100);
    c.bench_function("read_lines_100..200", |b| {
        b.iter(|| lines::read_lines(&path, 100, 200).unwrap());
    });
    c.bench_function("read_lines_50000..50100", |b| {
        b.iter(|| lines::read_lines(&path, 50_000, 50_100).unwrap());
    });
}

criterion_group!(benches, bench_construct_line_index, bench_read_lines_range);
criterion_main!(benches);
