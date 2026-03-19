use criterion::{criterion_group, criterion_main, Criterion};
use my_pet::sprite::{
    animation::AnimationState,
    behavior::{AnimTagMap, BehaviorAi, BehaviorState, Facing},
    sheet::load_embedded,
};

fn bench_animation_tick(c: &mut Criterion) {
    let sheet = load_embedded(
        include_bytes!("../assets/test_pet.json"),
        include_bytes!("../assets/test_pet.png"),
    )
    .unwrap();
    let tag = sheet.tags.first().map(|t| t.name.clone()).unwrap_or_default();
    let mut anim = AnimationState::new(tag);
    c.bench_function("animation_tick", |b| b.iter(|| anim.tick(&sheet, 16)));
}

fn bench_behavior_tick(c: &mut Criterion) {
    let tag_map = AnimTagMap {
        idle: "idle".into(),
        walk: "walk".into(),
        run: None,
        sit: None,
        sleep: None,
        wake: None,
        grabbed: None,
        petted: None,
        react: None,
        fall: None,
        thrown: None,
    };
    let mut ai = BehaviorAi::new();
    ai.state = BehaviorState::Walk { facing: Facing::Right, remaining_px: 10_000.0 };
    c.bench_function("behavior_tick", |b| {
        b.iter(|| {
            let mut x = 100i32;
            let mut y = 0i32;
            ai.tick(16, &mut x, &mut y, 1920, 32, 32, 100.0, 1000, &tag_map)
        })
    });
}

criterion_group!(animation_benches, bench_animation_tick, bench_behavior_tick);
criterion_main!(animation_benches);
