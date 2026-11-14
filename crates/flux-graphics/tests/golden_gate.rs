//! G4-гейт: golden-image сравнение с закоммиченным per-runner baseline.
//!
//! Гейт CPU-only и детерминированный: кадр строится формулой, baseline
//! лежит в `tests/data/golden_baseline.json`. Изменение baseline возможно
//! только с `visual_change = true` (PR «visual change»).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use flux_graphics::{GoldenError, GoldenSettings, GoldenStore, Rgba8Image};

const BASELINE: &str = include_str!("data/golden_baseline.json");
const RUNNER: &str = "windows-cpu";
const SIZE: u32 = 8;

fn frame() -> Rgba8Image {
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            pixels.push((x * 32) as u8);
            pixels.push((y * 32) as u8);
            pixels.push(((x + y) * 16) as u8);
            pixels.push(255);
        }
    }
    Rgba8Image::new(SIZE, SIZE, pixels).expect("frame")
}

fn store() -> GoldenStore {
    serde_json::from_str(BASELINE).expect("golden baseline")
}

#[test]
fn g4_baseline_matches_reference_frame() {
    let store = store();
    let verdict = store.compare(RUNNER, &frame()).expect("compare");
    assert!(
        verdict.passed,
        "G4: psnr={} ssim={}",
        verdict.psnr, verdict.ssim
    );
    assert_eq!(verdict.psnr, f64::INFINITY);
    assert!((verdict.ssim - 1.0).abs() < 1e-9);
}

#[test]
fn g4_perturbed_frame_fails_and_needs_visual_change() {
    let mut store = store();
    let mut pixels = frame().pixels;
    pixels[0] = pixels[0].wrapping_add(64);
    let changed = Rgba8Image::new(SIZE, SIZE, pixels).expect("changed");

    let verdict = store.compare(RUNNER, &changed).expect("compare");
    assert!(!verdict.passed, "изменённый кадр обязан краснить G4");

    let heatmap = store.diff(RUNNER, &changed).expect("diff");
    assert!(
        heatmap
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel[0] > 0 || pixel[1] > 0 || pixel[2] > 0),
        "heatmap обязан показать различия"
    );

    assert!(matches!(
        store.update(RUNNER, changed.clone(), GoldenSettings::default(), false),
        Err(GoldenError::VisualChangeRequired(_))
    ));
    store
        .update(RUNNER, changed, GoldenSettings::default(), true)
        .expect("visual change");
    assert_eq!(
        store.baseline(RUNNER).expect("baseline").image.pixel(0, 0)[0],
        frame().pixel(0, 0)[0].wrapping_add(64)
    );
}
