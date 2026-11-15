//! GPU-проходы кадра (ADR-0042): G-buffer → SSAO → lighting → bloom →
//! tonemap → TAA на реальном Vulkan + PSNR ≥ 60 дБ против CPU-эталона.
//!
//! Каждый эталон получает **readback предыдущего прохода**, поэтому гейт
//! проверяет математику шейдеров, а не квантование входов. Без Vulkan-устройства
//! тест пропускается, но `FLUX_REQUIRE_VULKAN=1` превращает отсутствие GPU
//! в панику (lavapipe-job).

#![cfg(feature = "vulkan")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use flux_graphics::{
    decode_rgba16f, encode_srgb_u8, oct_decode, psnr_bytes, psnr_channels, reference_bloom,
    reference_gbuffer, reference_lighting, reference_ssao, reference_taa, reference_tonemap,
    FrameParams, FrameStage, GpuFrame, RhiDevice, VulkanDevice, FRAME_PASS_COUNT, FRAME_PASS_NAMES,
};
use flux_math::Vec3;

const SIZE: u32 = 32;
const PSNR_MIN: f64 = 60.0;

fn runtime_device() -> Option<VulkanDevice> {
    match VulkanDevice::new() {
        Ok(device) => Some(device),
        Err(error) => {
            if std::env::var_os("FLUX_REQUIRE_VULKAN").is_some() {
                panic!("FLUX_REQUIRE_VULKAN is set, but Vulkan is unavailable: {error}");
            }
            eprintln!("skipping Vulkan frame test: {error}");
            None
        }
    }
}

fn decode_vectors(bytes: &[u8], count: usize) -> Vec<Vec3> {
    (0..count)
        .map(|index| {
            let pixel = decode_rgba16f(bytes, index);
            Vec3::new(pixel[0], pixel[1], pixel[2])
        })
        .collect()
}

fn decode_normals(bytes: &[u8], count: usize) -> Vec<Vec3> {
    (0..count)
        .map(|index| {
            let pixel = decode_rgba16f(bytes, index);
            oct_decode(pixel[0], pixel[1])
        })
        .collect()
}

fn flatten(vectors: &[Vec3]) -> Vec<f32> {
    let mut result = Vec::with_capacity(vectors.len() * 3);
    for vector in vectors {
        result.extend_from_slice(&[vector.x, vector.y, vector.z]);
    }
    result
}

fn psnr_vectors(reference: &[Vec3], actual: &[Vec3], peak: f32) -> f64 {
    psnr_channels(&flatten(reference), &flatten(actual), peak)
}

fn quantize(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8
}

fn peak_of(reference: &[Vec3]) -> f32 {
    reference
        .iter()
        .map(|vector| vector.x.max(vector.y).max(vector.z))
        .fold(1.0f32, f32::max)
}

#[test]
fn gpu_frame_passes_match_cpu_reference() {
    let Some(mut device) = runtime_device() else {
        return;
    };
    let before = device.validation_errors();
    let count = (SIZE * SIZE) as usize;
    let params = FrameParams::default();
    let mut frame = GpuFrame::new(&mut device, SIZE, SIZE).unwrap();

    frame.render(&mut device, &params).unwrap();

    // FrameStats по пассам: ровно один timestamp-диапазон на проход.
    let zones = frame.zones();
    assert_eq!(zones.len(), FRAME_PASS_COUNT);
    for (zone, name) in zones.iter().zip(FRAME_PASS_NAMES) {
        assert_eq!(zone.name, name);
        assert!(zone.end_ns >= zone.start_ns, "{name} end < start");
    }
    assert!(frame.frame_duration_ns() >= frame.pass_duration_ns("frame.gbuffer"));

    let albedo_bytes = frame.readback(&mut device, FrameStage::Albedo).unwrap();
    let normal_bytes = frame.readback(&mut device, FrameStage::Normal).unwrap();
    let orm_bytes = frame.readback(&mut device, FrameStage::Orm).unwrap();
    let depth_bytes = frame.readback(&mut device, FrameStage::Depth).unwrap();
    let ssao_bytes = frame.readback(&mut device, FrameStage::Ssao).unwrap();
    let hdr_bytes = frame.readback(&mut device, FrameStage::Hdr).unwrap();
    let bloom_bytes = frame.readback(&mut device, FrameStage::Bloom).unwrap();
    let tonemap_bytes = frame.readback(&mut device, FrameStage::Tonemap).unwrap();
    let taa_bytes = frame.readback(&mut device, FrameStage::Taa).unwrap();

    // G-buffer: RT0 — sRGB-байты (сравнение в кодированном пространстве,
    // иначе квантование sRGB ограничивает PSNR ~57 дБ в линейном).
    let reference = reference_gbuffer(SIZE, SIZE);
    let mut expected_albedo = Vec::with_capacity(count * 4);
    for albedo in &reference.albedo {
        expected_albedo.extend_from_slice(&[
            encode_srgb_u8(albedo.x),
            encode_srgb_u8(albedo.y),
            encode_srgb_u8(albedo.z),
            255,
        ]);
    }
    let psnr = psnr_bytes(&expected_albedo, &albedo_bytes);
    assert!(psnr >= PSNR_MIN, "albedo psnr={psnr:.2} dB");
    let psnr = psnr_vectors(
        &reference.normal,
        &decode_normals(&normal_bytes, count),
        1.0,
    );
    assert!(psnr >= PSNR_MIN, "normal psnr={psnr:.2} dB");
    let mut expected_orm = Vec::with_capacity(count * 4);
    for index in 0..count {
        expected_orm.push(quantize(reference.ao_seed[index]));
        expected_orm.push(quantize(reference.roughness[index]));
        expected_orm.push(quantize(reference.metallic[index]));
        expected_orm.push(0);
    }
    let psnr = psnr_bytes(&expected_orm, &orm_bytes);
    assert!(psnr >= PSNR_MIN, "orm psnr={psnr:.2} dB");
    let depth: Vec<f32> = depth_bytes
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect();
    let psnr = psnr_channels(&reference.depth, &depth, 1.0);
    assert!(psnr >= PSNR_MIN, "depth psnr={psnr:.2} dB");

    // SSAO считается от readback-глубины, как и на GPU; таргет RGBA16F.
    let ssao_reference = reference_ssao(SIZE, SIZE, &depth, params.ssao_radius);
    let ssao: Vec<f32> = (0..count)
        .map(|index| decode_rgba16f(&ssao_bytes, index)[0])
        .collect();
    let psnr = psnr_channels(&ssao_reference, &ssao, 1.0);
    assert!(psnr >= PSNR_MIN, "ssao psnr={psnr:.2} dB");

    // Lighting: входы — те же readback-байты, что сэмплирует GPU.
    let normal_oct: Vec<f32> = (0..count)
        .flat_map(|index| {
            let pixel = decode_rgba16f(&normal_bytes, index);
            [pixel[0], pixel[1]]
        })
        .collect();
    let hdr_reference = reference_lighting(
        SIZE,
        SIZE,
        &albedo_bytes,
        &normal_oct,
        &orm_bytes,
        &ssao,
        &params,
    );
    let hdr = decode_vectors(&hdr_bytes, count);
    let psnr = psnr_vectors(&hdr_reference, &hdr, peak_of(&hdr_reference));
    assert!(psnr >= PSNR_MIN, "lighting psnr={psnr:.2} dB");

    // Bloom от readback-HDR.
    let bloom_reference = reference_bloom(SIZE, SIZE, &hdr, params.bloom_threshold);
    let bloom = decode_vectors(&bloom_bytes, count);
    let psnr = psnr_vectors(&bloom_reference, &bloom, peak_of(&bloom_reference));
    assert!(psnr >= PSNR_MIN, "bloom psnr={psnr:.2} dB");

    // Tonemap от readback-HDR и bloom.
    let tonemap_reference = reference_tonemap(&hdr, &bloom, &params);
    let tonemap = decode_vectors(&tonemap_bytes, count);
    let psnr = psnr_vectors(&tonemap_reference, &tonemap, 1.0);
    assert!(psnr >= PSNR_MIN, "tonemap psnr={psnr:.2} dB");

    // Первый кадр TAA: история очищена в ноль.
    let history = vec![Vec3::ZERO; count];
    let taa_reference = reference_taa(&tonemap, &history, params.taa_alpha);
    let taa = decode_vectors(&taa_bytes, count);
    let psnr = psnr_vectors(&taa_reference, &taa, 1.0);
    assert!(psnr >= PSNR_MIN, "taa frame 1 psnr={psnr:.2} dB");

    // Второй кадр: история — выход первого, параметры изменены.
    let mut second = params;
    second.exposure = 0.5;
    second.sun = [0.3, -0.4, 0.6];
    second.taa_alpha = 0.35;
    frame.render(&mut device, &second).unwrap();
    let tonemap_bytes = frame.readback(&mut device, FrameStage::Tonemap).unwrap();
    let taa_bytes = frame.readback(&mut device, FrameStage::Taa).unwrap();
    let tonemap = decode_vectors(&tonemap_bytes, count);
    let taa_reference = reference_taa(&tonemap, &taa, second.taa_alpha);
    let taa = decode_vectors(&taa_bytes, count);
    let psnr = psnr_vectors(&taa_reference, &taa, 1.0);
    assert!(psnr >= PSNR_MIN, "taa frame 2 psnr={psnr:.2} dB");

    device.wait_idle().unwrap();
    assert_eq!(
        device.validation_errors(),
        before,
        "validation log is dirty"
    );

    frame.destroy(&mut device);
    device.wait_idle().unwrap();
    assert_eq!(
        device.live_resources(),
        0,
        "frame resources must be released"
    );
}
