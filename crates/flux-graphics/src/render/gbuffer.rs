//! G-buffer ABI (ADR-0017): форматы и семантика аттачментов.
//!
//! Layout — контракт между G-buffer-проходом и lighting; расширения
//! (clearcoat и т.п.) добавляются keyword-ом без изменения форматов.

use super::super::rhi::TextureFormat;

/// RT0: albedo, RGBA8 sRGB.
pub const ALBEDO_FORMAT: TextureFormat = TextureFormat::Rgba8Srgb;
/// RT1: oct-encoded normal, RGBA16F.
pub const NORMAL_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
/// RT2: ORM (occlusion/roughness/metallic) + bindless-индекс расширений.
pub const ORM_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;
/// Depth: D32 float, Reversed-Z.
pub const DEPTH_FORMAT: TextureFormat = TextureFormat::D32Float;

/// Число цветовых аттачментов G-buffer.
pub const COLOR_ATTACHMENTS: usize = 3;
/// Индекс RT0.
pub const ALBEDO_SLOT: u32 = 0;
/// Индекс RT1.
pub const NORMAL_SLOT: u32 = 1;
/// Индекс RT2.
pub const ORM_SLOT: u32 = 2;

/// Форматы G-buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GbufferLayout {
    /// Albedo.
    pub albedo: TextureFormat,
    /// Normal.
    pub normal: TextureFormat,
    /// ORM + bindless-индекс.
    pub orm: TextureFormat,
    /// Depth.
    pub depth: TextureFormat,
}

impl Default for GbufferLayout {
    fn default() -> Self {
        Self {
            albedo: ALBEDO_FORMAT,
            normal: NORMAL_FORMAT,
            orm: ORM_FORMAT,
            depth: DEPTH_FORMAT,
        }
    }
}

impl GbufferLayout {
    /// Цветовые аттачменты в порядке RT0..RT2.
    #[must_use]
    pub const fn color_formats(&self) -> [TextureFormat; COLOR_ATTACHMENTS] {
        [self.albedo, self.normal, self.orm]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_matches_adr_0017() {
        let layout = GbufferLayout::default();
        assert_eq!(layout.color_formats()[0], TextureFormat::Rgba8Srgb);
        assert_eq!(layout.color_formats()[1], TextureFormat::Rgba16Float);
        assert_eq!(layout.color_formats()[2], TextureFormat::Rgba8Unorm);
        assert_eq!(layout.depth, TextureFormat::D32Float);
        assert_eq!(COLOR_ATTACHMENTS, 3);
    }

    #[test]
    fn clearcoat_extension_does_not_change_layout() {
        // Расширения живут в bindless-индексе RT2, а не в новых аттачментах.
        let layout = GbufferLayout::default();
        assert_eq!(layout.color_formats().len(), COLOR_ATTACHMENTS);
        assert_eq!(ORM_SLOT, 2);
    }
}
