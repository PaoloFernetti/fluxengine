//! Пост-обработка (I5): эталонные функции bloom, TAA и SSAO.
//!
//! Это CPU-эталон для будущих GPU-проходов: формулы фиксируют контракт
//! (порог bloom, смешивание истории, фактор AO), а исполнение появится в
//! Render Graph после проходов I5+.

use flux_math::Vec3;

/// Порог bloom: всё, что ярче `threshold`, идёт в яркий проход.
#[must_use]
pub fn bloom_threshold(color: Vec3, threshold: f32) -> Vec3 {
    (color - Vec3::splat(threshold.max(0.0))).max(Vec3::ZERO)
}

/// Смешивает базовый цвет с bloom-вкладом.
#[must_use]
pub fn bloom_combine(base: Vec3, bloom: Vec3, intensity: f32) -> Vec3 {
    base + bloom * intensity.max(0.0)
}

/// TAA: смешивание истории и текущего кадра.
#[must_use]
pub fn taa_blend(history: Vec3, current: Vec3, alpha: f32) -> Vec3 {
    history.lerp(current, alpha.clamp(0.0, 1.0))
}

/// SSAO-фактор: доля сэмплов, перекрытых геометрией ближе центра.
///
/// Возвращает `1.0` (нет окклюзии) .. `0.0` (полностью перекрыт).
/// `sample_depths` — глубины соседей (Reversed-Z: больше — ближе).
#[must_use]
pub fn ssao_factor(center_depth: f32, sample_depths: &[f32], radius: f32) -> f32 {
    if sample_depths.is_empty() || radius <= 0.0 || center_depth <= 0.0 {
        return 1.0;
    }
    let occluded = sample_depths
        .iter()
        .filter(|&&depth| depth > center_depth && (depth - center_depth) <= radius)
        .count();
    1.0 - occluded as f32 / sample_depths.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bloom_only_keeps_bright_parts() {
        assert_eq!(bloom_threshold(Vec3::splat(0.2), 1.0), Vec3::ZERO);
        assert_eq!(bloom_threshold(Vec3::splat(1.5), 1.0), Vec3::splat(0.5));
    }

    #[test]
    fn bloom_combine_adds_energy() {
        let base = Vec3::splat(0.5);
        let result = bloom_combine(base, Vec3::splat(0.25), 2.0);
        assert!((result - Vec3::splat(1.0)).length() < 1e-6);
    }

    #[test]
    fn taa_blend_clamps_alpha() {
        let history = Vec3::ZERO;
        let current = Vec3::ONE;
        assert_eq!(taa_blend(history, current, 2.0), Vec3::ONE);
        assert_eq!(taa_blend(history, current, -1.0), Vec3::ZERO);
        assert_eq!(taa_blend(history, current, 0.5), Vec3::splat(0.5));
    }

    #[test]
    fn ssao_is_bounded() {
        assert_eq!(ssao_factor(1.0, &[], 0.1), 1.0);
        assert_eq!(ssao_factor(1.0, &[1.05, 1.05], 0.1), 0.0);
        assert_eq!(ssao_factor(1.0, &[0.5, 0.5], 0.1), 1.0);
        let partial = ssao_factor(1.0, &[1.05, 0.5], 0.1);
        assert!((0.0..=1.0).contains(&partial));
    }
}
