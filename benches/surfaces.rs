use criterion::{criterion_group, criterion_main, Criterion};
use my_pet::window::surfaces::{find_floor, SurfaceCache};

fn bench_find_floor_cold(c: &mut Criterion) {
    let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
    let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
    // Reduced sample size: EnumWindows is a blocking syscall with OS scheduling jitter.
    c.bench_function("find_floor_cold", |b| {
        b.iter(|| {
            // Re-expire cache on every iteration to force EnumWindows each time.
            let mut cache = SurfaceCache::default();
            find_floor(100, 0, 32, 32, screen_w, screen_h, &mut cache)
        })
    });
}

fn bench_find_floor_cached(c: &mut Criterion) {
    let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
    let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
    let mut cache = SurfaceCache::default();
    // Warm the cache once before benchmarking.
    find_floor(100, 0, 32, 32, screen_w, screen_h, &mut cache);
    c.bench_function("find_floor_cached", |b| {
        b.iter(|| find_floor(100, 0, 32, 32, screen_w, screen_h, &mut cache))
    });
}

criterion_group! {
    name = surfaces_benches;
    config = Criterion::default().sample_size(20);
    targets = bench_find_floor_cold, bench_find_floor_cached
}
criterion_main!(surfaces_benches);
