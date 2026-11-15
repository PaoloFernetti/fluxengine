//! G4/4.5: бюджет extract на эталонной сцене (10 000 инстансов).
//!
//! Метод: одна и та же сцена, camera-relative позиции и единичные AABB;
//! измеряется `RenderScene::extract` (интерполяция + culling). Порог приёмки
//! 4.5 — ≤ 1 мс; бенч печатает фактическое время, тест его не проверяет
//! (тайминги в CI нестабильны).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use flux_graphics::{Camera, RenderScene};
use flux_math::{Frustum, Vec3};

/// Эталонная сцена: сетка 100 × 100, половина инстансов за камерой.
fn reference_scene() -> RenderScene {
    const SIDE: i32 = 100;
    let mut scene = RenderScene::new();
    for y in 0..SIDE {
        for x in 0..SIDE {
            let offset_x = (x as f32 - SIDE as f32 * 0.5) * 2.0;
            let offset_y = (y as f32 - SIDE as f32 * 0.5) * 2.0;
            // Чётные — перед камерой, нечётные — за ней: culling отсекает ~50%.
            let z = if (x + y) % 2 == 0 { -10.0 } else { 10.0 };
            let position = Vec3::new(offset_x, offset_y, z);
            scene.push(position, position, 0, 0);
        }
    }
    scene
}

fn bench_extract(criterion: &mut Criterion) {
    let camera = Camera {
        far: 1_000.0,
        ..Camera::default()
    };
    let frustum = Frustum::from_view_projection(camera.view_projection());
    let mut scene = reference_scene();
    assert_eq!(scene.len(), 10_000);

    let mut group = criterion.benchmark_group("extract");
    group.bench_function("10k_interpolate_and_cull", |bencher| {
        bencher.iter(|| {
            let stats = scene.extract(black_box(0.5), black_box(&frustum));
            black_box(stats);
        });
    });
    group.finish();

    // Диагностика: сколько инстансов проходит фрустум на эталонной сцене.
    let stats = scene.extract(0.5, &frustum);
    assert_eq!(stats.instances, 10_000);
    assert_eq!(stats.visible + stats.culled, stats.instances);
}

criterion_group!(benches, bench_extract);
criterion_main!(benches);
