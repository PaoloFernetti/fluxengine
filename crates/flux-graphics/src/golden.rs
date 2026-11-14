//! Golden-image харнесс (ADR-0021): PSNR/SSIM, per-runner baselines, дифф.
//!
//! Сравнение детерминировано и не зависит от GPU: бейзлайны хранятся
//! per-runner (драйвер/ОС), а обновление существующего бейзлайна требует
//! явного флага `visual_change` — правило «golden меняется только через PR
//! visual change».

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Ошибки golden-image-сравнения.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum GoldenError {
    /// Некорректное изображение (размеры/длина буфера).
    #[error("некорректное изображение: {0}")]
    InvalidImage(String),
    /// Размеры reference и actual не совпадают.
    #[error("размеры не совпадают: reference {reference:?}, actual {actual:?}")]
    DimensionMismatch {
        /// Размер reference.
        reference: (u32, u32),
        /// Размер actual.
        actual: (u32, u32),
    },
    /// Для раннера нет бейзлайна.
    #[error("нет golden-бейзлайна для раннера {0}")]
    MissingBaseline(String),
    /// Обновление бейзлайна без PR «visual change».
    #[error("изменение golden требует PR visual change: {0}")]
    VisualChangeRequired(String),
}

/// RGBA8-изображение (sRGB, как в swapchain).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgba8Image {
    /// Ширина.
    pub width: u32,
    /// Высота.
    pub height: u32,
    /// Пиксели, `width * height * 4` байт.
    pub pixels: Vec<u8>,
}

impl Rgba8Image {
    /// Создаёт изображение с проверкой длины буфера.
    ///
    /// # Errors
    ///
    /// [`GoldenError::InvalidImage`] — нулевой размер или неверная длина.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, GoldenError> {
        if width == 0 || height == 0 {
            return Err(GoldenError::InvalidImage("нулевой размер".to_owned()));
        }
        let expected = width as usize * height as usize * 4;
        if pixels.len() != expected {
            return Err(GoldenError::InvalidImage(format!(
                "ожидалось {expected} байт, получено {}",
                pixels.len()
            )));
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Изображение, заполненное цветом.
    #[must_use]
    pub fn filled(width: u32, height: u32, color: [u8; 4]) -> Self {
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width as usize * height as usize {
            pixels.extend_from_slice(&color);
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    /// Пиксель по координатам.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let index = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.pixels[index],
            self.pixels[index + 1],
            self.pixels[index + 2],
            self.pixels[index + 3],
        ]
    }

    /// Число пикселей.
    #[must_use]
    pub fn len(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Пустое ли изображение (всегда `false` после `new`).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Пороговые значения сравнения.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GoldenSettings {
    /// Минимальный PSNR, дБ (план: ≥ 60).
    pub psnr: f64,
    /// Минимальный SSIM (0..=1).
    pub ssim: f64,
}

impl Default for GoldenSettings {
    fn default() -> Self {
        Self {
            psnr: 60.0,
            ssim: 0.99,
        }
    }
}

impl GoldenSettings {
    /// Санитизирует пороги: PSNR ≥ 0, SSIM в `0..=1`.
    #[must_use]
    pub fn validated(mut self) -> Self {
        if !self.psnr.is_finite() || self.psnr < 0.0 {
            self.psnr = 0.0;
        }
        if !self.ssim.is_finite() {
            self.ssim = 0.0;
        }
        self.ssim = self.ssim.clamp(0.0, 1.0);
        self
    }
}

fn check_same_size(reference: &Rgba8Image, actual: &Rgba8Image) -> Result<(), GoldenError> {
    if reference.width != actual.width || reference.height != actual.height {
        return Err(GoldenError::DimensionMismatch {
            reference: (reference.width, reference.height),
            actual: (actual.width, actual.height),
        });
    }
    Ok(())
}

/// PSNR по RGB-каналам (alpha игнорируется).
///
/// # Errors
///
/// [`GoldenError::DimensionMismatch`] — разные размеры.
pub fn psnr(reference: &Rgba8Image, actual: &Rgba8Image) -> Result<f64, GoldenError> {
    check_same_size(reference, actual)?;
    let mut squared = 0.0_f64;
    let mut samples = 0_u64;
    for (index, (a, b)) in reference
        .pixels
        .chunks_exact(4)
        .zip(actual.pixels.chunks_exact(4))
        .enumerate()
    {
        let _ = index;
        for channel in 0..3 {
            let diff = f64::from(a[channel]) - f64::from(b[channel]);
            squared += diff * diff;
            samples += 1;
        }
    }
    if squared == 0.0 {
        return Ok(f64::INFINITY);
    }
    let mse = squared / samples as f64;
    Ok(10.0 * (255.0_f64 * 255.0 / mse).log10())
}

/// Глобальный SSIM по luma (BT.709).
///
/// # Errors
///
/// [`GoldenError::DimensionMismatch`] — разные размеры.
pub fn ssim(reference: &Rgba8Image, actual: &Rgba8Image) -> Result<f64, GoldenError> {
    check_same_size(reference, actual)?;
    let count = reference.len() as f64;
    let mut sum_x = 0.0_f64;
    let mut sum_y = 0.0_f64;
    for (a, b) in reference
        .pixels
        .chunks_exact(4)
        .zip(actual.pixels.chunks_exact(4))
    {
        sum_x += luma(a);
        sum_y += luma(b);
    }
    let mean_x = sum_x / count;
    let mean_y = sum_y / count;

    let mut var_x = 0.0_f64;
    let mut var_y = 0.0_f64;
    let mut covariance = 0.0_f64;
    for (a, b) in reference
        .pixels
        .chunks_exact(4)
        .zip(actual.pixels.chunks_exact(4))
    {
        let dx = luma(a) - mean_x;
        let dy = luma(b) - mean_y;
        var_x += dx * dx;
        var_y += dy * dy;
        covariance += dx * dy;
    }
    var_x /= count;
    var_y /= count;
    covariance /= count;

    let c1 = (0.01 * 255.0_f64).powi(2);
    let c2 = (0.03 * 255.0_f64).powi(2);
    let numerator = (2.0 * mean_x * mean_y + c1) * (2.0 * covariance + c2);
    let denominator = (mean_x * mean_x + mean_y * mean_y + c1) * (var_x + var_y + c2);
    Ok((numerator / denominator).clamp(-1.0, 1.0))
}

fn luma(pixel: &[u8]) -> f64 {
    0.2126 * f64::from(pixel[0]) + 0.7152 * f64::from(pixel[1]) + 0.0722 * f64::from(pixel[2])
}

/// Тепловая карта абсолютной разницы (усиление ×8), alpha = 255.
///
/// # Errors
///
/// [`GoldenError::DimensionMismatch`] — разные размеры.
pub fn diff_heatmap(
    reference: &Rgba8Image,
    actual: &Rgba8Image,
) -> Result<Rgba8Image, GoldenError> {
    check_same_size(reference, actual)?;
    let mut pixels = Vec::with_capacity(reference.pixels.len());
    for (a, b) in reference
        .pixels
        .chunks_exact(4)
        .zip(actual.pixels.chunks_exact(4))
    {
        for channel in 0..3 {
            let diff = a[channel].abs_diff(b[channel]);
            pixels.push(diff.saturating_mul(8));
        }
        pixels.push(255);
    }
    Ok(Rgba8Image {
        width: reference.width,
        height: reference.height,
        pixels,
    })
}

/// Бейзлайн одного раннера.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GoldenBaseline {
    /// Эталонное изображение.
    pub image: Rgba8Image,
    /// Минимальный PSNR.
    pub psnr_threshold: f64,
    /// Минимальный SSIM.
    pub ssim_threshold: f64,
}

/// Результат сравнения с бейзлайном.
#[derive(Clone, Debug, PartialEq)]
pub struct GoldenVerdict {
    /// Раннер.
    pub runner: String,
    /// Фактический PSNR.
    pub psnr: f64,
    /// Фактический SSIM.
    pub ssim: f64,
    /// Прошёл ли гейт.
    pub passed: bool,
}

/// Хранилище бейзлайнов per-runner.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GoldenStore {
    baselines: BTreeMap<String, GoldenBaseline>,
}

impl GoldenStore {
    /// Пустое хранилище.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Бейзлайн раннера.
    #[must_use]
    pub fn baseline(&self, runner: &str) -> Option<&GoldenBaseline> {
        self.baselines.get(runner)
    }

    /// Число бейзлайнов.
    #[must_use]
    pub fn len(&self) -> usize {
        self.baselines.len()
    }

    /// Пусто ли хранилище.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.baselines.is_empty()
    }

    /// Сравнивает изображение с бейзлайном раннера.
    ///
    /// # Errors
    ///
    /// [`GoldenError::MissingBaseline`] — бейзлайна нет;
    /// [`GoldenError::DimensionMismatch`] — разные размеры.
    pub fn compare(&self, runner: &str, actual: &Rgba8Image) -> Result<GoldenVerdict, GoldenError> {
        let baseline = self
            .baselines
            .get(runner)
            .ok_or_else(|| GoldenError::MissingBaseline(runner.to_owned()))?;
        let psnr_value = psnr(&baseline.image, actual)?;
        let ssim_value = ssim(&baseline.image, actual)?;
        let passed = psnr_value >= baseline.psnr_threshold && ssim_value >= baseline.ssim_threshold;
        Ok(GoldenVerdict {
            runner: runner.to_owned(),
            psnr: psnr_value,
            ssim: ssim_value,
            passed,
        })
    }

    /// Тепловая карта различий с бейзлайном.
    ///
    /// # Errors
    ///
    /// [`GoldenError::MissingBaseline`] — бейзлайна нет;
    /// [`GoldenError::DimensionMismatch`] — разные размеры.
    pub fn diff(&self, runner: &str, actual: &Rgba8Image) -> Result<Rgba8Image, GoldenError> {
        let baseline = self
            .baselines
            .get(runner)
            .ok_or_else(|| GoldenError::MissingBaseline(runner.to_owned()))?;
        diff_heatmap(&baseline.image, actual)
    }

    /// Устанавливает бейзлайн. Для существующего раннера требуется
    /// `visual_change = true` (правило golden-change).
    ///
    /// # Errors
    ///
    /// [`GoldenError::VisualChangeRequired`] — бейзлайн уже есть, а PR не
    /// помечен как visual change.
    pub fn update(
        &mut self,
        runner: impl Into<String>,
        image: Rgba8Image,
        settings: GoldenSettings,
        visual_change: bool,
    ) -> Result<(), GoldenError> {
        let runner = runner.into();
        if self.baselines.contains_key(&runner) && !visual_change {
            return Err(GoldenError::VisualChangeRequired(runner));
        }
        let settings = settings.validated();
        self.baselines.insert(
            runner,
            GoldenBaseline {
                image,
                psnr_threshold: settings.psnr,
                ssim_threshold: settings.ssim,
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, value: u8) -> Rgba8Image {
        Rgba8Image::filled(width, height, [value, value, value, 255])
    }

    #[test]
    fn image_validates_size() {
        assert!(Rgba8Image::new(2, 2, vec![0; 16]).is_ok());
        assert!(matches!(
            Rgba8Image::new(0, 2, Vec::new()),
            Err(GoldenError::InvalidImage(_))
        ));
        assert!(matches!(
            Rgba8Image::new(2, 2, vec![0; 15]),
            Err(GoldenError::InvalidImage(_))
        ));
    }

    #[test]
    fn identical_images_are_perfect() {
        let a = image(4, 4, 128);
        let b = image(4, 4, 128);
        assert_eq!(psnr(&a, &b).expect("psnr"), f64::INFINITY);
        assert!((ssim(&a, &b).expect("ssim") - 1.0).abs() < 1e-9);
        let heatmap = diff_heatmap(&a, &b).expect("heatmap");
        assert!(heatmap.pixels.chunks_exact(4).all(|pixel| pixel[0] == 0));
    }

    #[test]
    fn psnr_drops_with_difference() {
        let a = image(4, 4, 0);
        let mut pixels = a.pixels.clone();
        pixels[0] = 255;
        let b = Rgba8Image::new(4, 4, pixels).expect("image");
        let value = psnr(&a, &b).expect("psnr");
        assert!(value.is_finite() && value < 60.0);
        assert!(ssim(&a, &b).expect("ssim") < 1.0);
    }

    #[test]
    fn dimension_mismatch_is_rejected() {
        let a = image(2, 2, 0);
        let b = image(3, 2, 0);
        assert!(matches!(
            psnr(&a, &b),
            Err(GoldenError::DimensionMismatch { .. })
        ));
        assert!(matches!(
            ssim(&a, &b),
            Err(GoldenError::DimensionMismatch { .. })
        ));
        assert!(matches!(
            diff_heatmap(&a, &b),
            Err(GoldenError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn heatmap_amplifies_difference() {
        let a = image(1, 1, 0);
        let mut pixels = a.pixels.clone();
        pixels[0] = 1;
        let b = Rgba8Image::new(1, 1, pixels).expect("image");
        let heatmap = diff_heatmap(&a, &b).expect("heatmap");
        assert_eq!(heatmap.pixel(0, 0), [8, 0, 0, 255]);
    }

    #[test]
    fn store_compares_per_runner() {
        let mut store = GoldenStore::new();
        let settings = GoldenSettings::default();
        store
            .update("windows-vulkan", image(4, 4, 64), settings, false)
            .expect("first baseline");
        assert!(
            store
                .compare("windows-vulkan", &image(4, 4, 64))
                .expect("compare")
                .passed
        );
        assert!(matches!(
            store.compare("linux-vulkan", &image(4, 4, 64)),
            Err(GoldenError::MissingBaseline(_))
        ));

        let mut pixels = image(4, 4, 64).pixels;
        pixels[0] = 255;
        let changed = Rgba8Image::new(4, 4, pixels).expect("image");
        let verdict = store.compare("windows-vulkan", &changed).expect("compare");
        assert!(!verdict.passed);
    }

    #[test]
    fn golden_change_requires_visual_change_flag() {
        let mut store = GoldenStore::new();
        let settings = GoldenSettings::default();
        store
            .update("runner", image(2, 2, 10), settings, false)
            .expect("first baseline");
        assert!(matches!(
            store.update("runner", image(2, 2, 20), settings, false),
            Err(GoldenError::VisualChangeRequired(_))
        ));
        store
            .update("runner", image(2, 2, 20), settings, true)
            .expect("visual change");
        assert_eq!(
            store
                .baseline("runner")
                .expect("baseline")
                .image
                .pixel(0, 0),
            [20, 20, 20, 255]
        );
    }

    #[test]
    fn store_roundtrips_through_json() {
        let mut store = GoldenStore::new();
        store
            .update("runner", image(2, 2, 42), GoldenSettings::default(), false)
            .expect("baseline");
        let json = serde_json::to_string(&store).expect("serialize");
        let again: GoldenStore = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(store, again);
    }

    #[test]
    fn thresholds_are_sanitized() {
        let settings = GoldenSettings {
            psnr: f64::NAN,
            ssim: 2.0,
        }
        .validated();
        assert_eq!(settings.psnr, 0.0);
        assert_eq!(settings.ssim, 1.0);
    }
}
