//! PBR-модель (ADR-0017): GGX Smith height-correlated, multiscatter, metallic.
//!
//! Функции — эталон для GPU-шейдеров и CPU-тестов: та же модель должна
//! использоваться в G-buffer lighting. Энергия спекуляра компенсируется
//! множителем multiscatter (Karis), диффуз — Lambert.

use flux_math::Vec3;

/// Материал PBR.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Базовый цвет (linear).
    pub base_color: Vec3,
    /// Металличность `0..=1`.
    pub metallic: f32,
    /// Шероховатость `0.04..=1`.
    pub roughness: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: Vec3::splat(0.8),
            metallic: 0.0,
            roughness: 0.5,
        }
    }
}

impl Material {
    /// Материал с санитизацией metallic/roughness.
    #[must_use]
    pub fn new(base_color: Vec3, metallic: f32, roughness: f32) -> Self {
        Self {
            base_color,
            metallic: metallic.clamp(0.0, 1.0),
            roughness: roughness.clamp(0.04, 1.0),
        }
    }

    /// Диэлектрик.
    #[must_use]
    pub fn dielectric(base_color: Vec3, roughness: f32) -> Self {
        Self::new(base_color, 0.0, roughness)
    }

    /// Металл.
    #[must_use]
    pub fn metal(base_color: Vec3, roughness: f32) -> Self {
        Self::new(base_color, 1.0, roughness)
    }

    /// F0: 0.04 для диэлектрика, base_color для металла.
    #[must_use]
    pub fn f0(self) -> Vec3 {
        Vec3::splat(0.04).lerp(self.base_color, self.metallic)
    }
}

/// GGX/Trowbridge-Reitz distribution.
#[must_use]
pub fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness.clamp(0.04, 1.0);
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    a2 / (std::f32::consts::PI * d * d).max(1e-7)
}

/// Smith height-correlated visibility (`G / (4 n·l n·v)`).
#[must_use]
pub fn visibility_smith_height_correlated(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let a = roughness.clamp(0.04, 1.0);
    let a2 = a * a;
    let gv = n_dot_l * (n_dot_v * n_dot_v * (1.0 - a2) + a2).sqrt();
    let gl = n_dot_v * (n_dot_l * n_dot_l * (1.0 - a2) + a2).sqrt();
    0.5 / (gv + gl).max(1e-5)
}

/// Schlick Fresnel.
#[must_use]
pub fn fresnel_schlick(cos_theta: f32, f0: Vec3) -> Vec3 {
    let factor = (1.0 - cos_theta).clamp(0.0, 1.0).powi(5);
    f0 + (Vec3::ONE - f0) * factor
}

/// Multiscatter-компенсация энергии спекуляра (Karis).
#[must_use]
pub fn multiscatter_energy(f0: Vec3, roughness: f32, n_dot_v: f32) -> Vec3 {
    let e = fresnel_schlick(n_dot_v.clamp(0.0, 1.0), f0);
    Vec3::ONE + f0 * (1.0 / e.max(Vec3::splat(1e-4)) - Vec3::ONE) * (1.0 - roughness)
}

/// Прямое освещение: `(diffuse + specular) * radiance * n·l`.
#[must_use]
pub fn evaluate_direct(
    material: Material,
    normal: Vec3,
    view: Vec3,
    light: Vec3,
    radiance: Vec3,
) -> Vec3 {
    let n = normal.normalize_or_zero();
    let v = view.normalize_or_zero();
    let l = light.normalize_or_zero();
    let n_dot_l = n.dot(l).max(0.0);
    if n_dot_l <= 0.0 {
        return Vec3::ZERO;
    }
    let n_dot_v = n.dot(v).max(1e-4);
    let h = (v + l).normalize_or_zero();
    let n_dot_h = n.dot(h).max(0.0);
    let v_dot_h = v.dot(h).max(0.0);

    let f0 = material.f0();
    let fresnel = fresnel_schlick(v_dot_h, f0);
    let specular = distribution_ggx(n_dot_h, material.roughness)
        * visibility_smith_height_correlated(n_dot_v, n_dot_l, material.roughness)
        * fresnel
        * multiscatter_energy(f0, material.roughness, n_dot_v);
    let diffuse = (Vec3::ONE - fresnel)
        * (1.0 - material.metallic)
        * material.base_color
        * std::f32::consts::FRAC_1_PI;
    (diffuse + specular) * radiance * n_dot_l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_surfaces_have_sharper_highlight() {
        let smooth = distribution_ggx(1.0, 0.1);
        let rough = distribution_ggx(1.0, 0.9);
        assert!(smooth > rough);
        assert!(smooth.is_finite() && rough.is_finite());
    }

    #[test]
    fn fresnel_endpoints_match_f0_and_one() {
        let f0 = Vec3::new(0.04, 0.1, 0.2);
        let at_normal = fresnel_schlick(1.0, f0);
        assert!((at_normal - f0).length() < 1e-5);
        let at_grazing = fresnel_schlick(0.0, f0);
        assert!((at_grazing - Vec3::ONE).length() < 1e-5);
    }

    #[test]
    fn metal_specular_is_tinted_by_base_color() {
        // Металл: F0 == base_color, диффузной доли нет.
        // Multiscatter-компенсация (Karis) хроматична, поэтому отношения
        // каналов не равны base_color — проверяем само тонирование.
        let red_metal = Material::metal(Vec3::new(1.0, 0.0, 0.0), 0.3);
        let red = evaluate_direct(red_metal, Vec3::Z, Vec3::Z, Vec3::Z, Vec3::splat(1.0));
        assert!(red.x > 0.0);
        assert!(red.y.abs() < 1e-6 && red.z.abs() < 1e-6);

        // Диэлектрик того же цвета: нейтральный F0 = 0.04 плюс диффуз —
        // в G/B появляется энергия, которой у металла нет.
        let red_dielectric = Material::dielectric(Vec3::new(1.0, 0.0, 0.0), 0.3);
        let lit = evaluate_direct(red_dielectric, Vec3::Z, Vec3::Z, Vec3::Z, Vec3::splat(1.0));
        assert!(lit.y > red.y);
        assert!(lit.z > red.z);

        // Нейтральный металл даёт нейтральный отклик.
        let gray = evaluate_direct(
            Material::metal(Vec3::splat(0.5), 0.3),
            Vec3::Z,
            Vec3::Z,
            Vec3::Z,
            Vec3::splat(1.0),
        );
        assert!((gray.x - gray.y).abs() < 1e-6 && (gray.y - gray.z).abs() < 1e-6);
    }

    #[test]
    fn direct_lighting_is_finite_and_non_negative() {
        for roughness in [0.04, 0.2, 0.5, 0.9, 1.0] {
            for angle in [0.0_f32, 0.5, 1.0, 1.5] {
                let material = Material::new(Vec3::splat(0.5), 0.5, roughness);
                let normal = Vec3::Z;
                let view = Vec3::new(angle.sin(), 0.0, angle.cos());
                let light = Vec3::new(0.3, 0.2, 0.9);
                let color = evaluate_direct(material, normal, view, light, Vec3::splat(3.0));
                assert!(color.is_finite());
                assert!(color.x >= 0.0 && color.y >= 0.0 && color.z >= 0.0);
            }
        }
    }

    #[test]
    fn material_clamps_parameters() {
        let material = Material::new(Vec3::ONE, 2.0, -1.0);
        assert_eq!(material.metallic, 1.0);
        assert_eq!(material.roughness, 0.04);
    }
}
