use criterion::{criterion_group, criterion_main, Criterion};
use ferrite_core::geometry::PetGeom;
use my_pet::window::surfaces::{find_floor, SurfaceCache};

fn bench_find_floor_cold(c: &mut Criterion) {
    let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
    let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
    let geom = PetGeom { x: 100, y: 0, w: 32, h: 32, baseline_offset: 0 };
    // Reduced sample size: EnumWindows is a blocking syscall with OS scheduling jitter.
    c.bench_function("find_floor_cold", |b| {
        b.iter(|| {
            let mut cache = SurfaceCache::default();
            find_floor(&geom, screen_w, screen_h, &mut cache)
        })
    });
}

fn bench_find_floor_cached(c: &mut Criterion) {
    let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
    let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
    let geom = PetGeom { x: 100, y: 0, w: 32, h: 32, baseline_offset: 0 };
    let mut cache = SurfaceCache::default();
    find_floor(&geom, screen_w, screen_h, &mut cache);
    c.bench_function("find_floor_cached", |b| {
        b.iter(|| find_floor(&geom, screen_w, screen_h, &mut cache))
    });
}

criterion_group! {
    name = surfaces_benches;
    config = Criterion::default().sample_size(20);
    targets = bench_find_floor_cold, bench_find_floor_cached
}
criterion_main!(surfaces_benches);
