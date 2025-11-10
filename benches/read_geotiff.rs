use criterion::{Criterion, criterion_group, criterion_main};

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_geotiff");

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
