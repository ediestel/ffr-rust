use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use ffr_core::read;
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn make_file(dir: &std::path::Path, name: &str, line_count: usize, line_len: usize) -> String {
    let path = dir.join(name);
    let mut f = fs::File::create(&path).unwrap();
    let line = "a".repeat(line_len);
    for _ in 0..line_count {
        writeln!(f, "{}", line).unwrap();
    }
    path.to_string_lossy().into_owned()
}

fn bench_read_chunk(c: &mut Criterion) {
    let tmp = TempDir::new().unwrap();
    let mut group = c.benchmark_group("read_chunk");

    for &(kb, line_len) in &[(64usize, 80usize), (512, 120), (4096, 160)] {
        let line_count = (kb * 1024) / (line_len + 1);
        let path = make_file(tmp.path(), &format!("f{kb}.txt"), line_count, line_len);
        let chunk_bytes = 64 * 1024;

        group.throughput(Throughput::Bytes((kb * 1024) as u64));
        group.bench_with_input(
            BenchmarkId::new("read_chunk_0", format!("{kb}KB")),
            &path,
            |b, p| {
                b.iter(|| read::read_chunk(p, 0, chunk_bytes).unwrap());
            },
        );
    }
    group.finish();
}

fn bench_read_bytes(c: &mut Criterion) {
    let tmp = TempDir::new().unwrap();
    let path = make_file(tmp.path(), "bytes.txt", 10_000, 100);
    c.bench_function("read_bytes_4kb", |b| {
        b.iter(|| read::read_bytes(&path, 0, 4096).unwrap());
    });
}

criterion_group!(benches, bench_read_chunk, bench_read_bytes);
criterion_main!(benches);
