//! Тон-маппинг (ADR-0018): ACES и AgX.
//!
//! Обе кривые переводят HDR в `0..=1`; выбор — художественный ABI контента,
//! поэтому он вынесен в [`Tonemap`].

use flux_math::Vec3;

/// Семейство тон-маппинга.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tonemap {
    /// ACES (Narkowicz fit).
    #[default]
    Aces,
    /// AgX (приближение для CPU-эталона).
    Agx,
}

impl Tonemap {
    /// Применяет экспозицию (в стопах) и тон-маппинг.
    #[must_use]
    pub fn apply(self, color: Vec3, exposure: f32) -> Vec3 {
        let exposed = color * 2.0_f32.powf(exposure);
        match self {
            Self::Aces => aces(exposed),
            Self::Agx => agx(exposed),
        }
    }
}

/// ACES filmic (Narkowicz).
#[must_use]
pub fn aces(color: Vec3) -> Vec3 {
    let x = color.max(Vec3::ZERO);
    let numerator = x * (x * 2.51 + Vec3::splat(0.03));
    let denominator = x * (x * 2.43 + Vec3::splat(0.59)) + Vec3::splat(0.14);
    (numerator / denominator.max(Vec3::splat(1e-6))).clamp(Vec3::ZERO, Vec3::ONE)
}

/// AgX-подобная кривая: log-нормализация + S-контраст.
#[must_use]
pub fn agx(color: Vec3) -> Vec3 {
    let x = color.max(Vec3::splat(1e-6));
    let log = Vec3::new(x.x.log2(), x.y.log2(), x.z.log2());
    let normalized =
        ((log + Vec3::splat(12.473_93)) / Vec3::splat(16.0)).clamp(Vec3::ZERO, Vec3::ONE);
    (normalized * normalized * (Vec3::splat(3.0) - 2.0 * normalized)).clamp(Vec3::ZERO, Vec3::ONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_stays_black() {
        assert_eq!(aces(Vec3::ZERO), Vec3::ZERO);
        assert_eq!(agx(Vec3::ZERO), Vec3::ZERO);
    }

    #[test]
    fn output_is_bounded() {
        for value in [0.1, 1.0, 10.0, 1000.0] {
            let color = Vec3::splat(value);
            for tonemap in [Tonemap::Aces, Tonemap::Agx] {
                let mapped = tonemap.apply(color, 0.0);
                assert!(mapped.x.is_finite());
                assert!((0.0..=1.0).contains(&mapped.x));
            }
        }
    }

    #[test]
    fn curves_are_monotonic() {
        let mut previous = -1.0;
        for value in [0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0] {
            let mapped = aces(Vec3::splat(value)).x;
            assert!(mapped >= previous);
            previous = mapped;
        }
        let mut previous = -1.0;
        for value in [0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0] {
            let mapped = agx(Vec3::splat(value)).x;
            assert!(mapped >= previous);
            previous = mapped;
        }
    }

    #[test]
    fn exposure_brightens() {
        let color = Vec3::splat(0.2);
        let base = Tonemap::Aces.apply(color, 0.0);
        let brighter = Tonemap::Aces.apply(color, 1.0);
        assert!(brighter.x > base.x);
    }
}
