use criterion::{criterion_group, criterion_main, Criterion};
use my_pet::sprite::sheet::load_embedded;

fn bench_blit_frame(c: &mut Criterion, scale: u32, label: &str) {
    let sheet = load_embedded(
        include_bytes!("../assets/test_pet.json"),
        include_bytes!("../assets/test_pet.png"),
    )
    .unwrap();
    let f = &sheet.frames[0];
    let mut buf = Vec::new();
    c.bench_function(label, |b| {
        b.iter(|| {
            my_pet::window::blender::blit_frame(
                &sheet.image, f.x, f.y, f.w, f.h, &mut buf, scale, false,
            )
        })
    });
}

fn bench_blit_1x(c: &mut Criterion) {
    bench_blit_frame(c, 1, "blit_frame_1x");
}
fn bench_blit_2x(c: &mut Criterion) {
    bench_blit_frame(c, 2, "blit_frame_2x");
}
fn bench_blit_4x(c: &mut Criterion) {
    bench_blit_frame(c, 4, "blit_frame_4x");
}

criterion_group!(render_benches, bench_blit_1x, bench_blit_2x, bench_blit_4x);
criterion_main!(render_benches);
