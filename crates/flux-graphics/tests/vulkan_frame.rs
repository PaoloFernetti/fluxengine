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
    shadow_view_projection, Camera, FrameFeatures, FrameParams, FrameStage, GpuFrame, RenderScene,
    RhiDevice, SceneGeometry, VulkanDevice, FRAME_PASS_COUNT, FRAME_PASS_NAMES, MATERIAL_PALETTE,
    SHADOW_LIGHT_DIR, SHADOW_MAP_SIZE,
};
use flux_math::{Frustum, Vec3};

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

#[test]
fn scene_geometry_fills_gbuffer() {
    let Some(mut device) = runtime_device() else {
        return;
    };
    let before = device.validation_errors();
    let size = 32u32;
    let camera = Camera {
        aspect: 1.0,
        ..Camera::default()
    };

    let mut scene = RenderScene::new();
    let position = Vec3::new(0.0, 0.0, -2.0);
    scene.push(position, position, 0, 0);
    scene.extract(
        1.0,
        &Frustum::from_view_projection(camera.view_projection()),
    );
    assert_eq!(scene.visible_indices(), &[0]);

    let mut geometry = SceneGeometry::new(&mut device, 4).unwrap();
    geometry.upload(&mut device, &scene).unwrap();
    assert_eq!(geometry.instance_count(), 1);

    let mut frame =
        GpuFrame::with_features(&mut device, size, size, FrameFeatures::none()).unwrap();
    frame
        .render_scene(&mut device, &FrameParams::default(), &geometry, &camera)
        .unwrap();
    assert_eq!(frame.zones().len(), 3);

    // Центр куба: albedo материала 0 (RGBA8 sRGB) и ORM (RGBA8 UNORM).
    let center = ((size / 2) * size + size / 2) as usize * 4;
    let albedo = frame.readback(&mut device, FrameStage::Albedo).unwrap();
    let expected = MATERIAL_PALETTE[0];
    assert_eq!(albedo[center], quantize(expected[0]));
    assert!(albedo[center + 1] <= 2, "green={}", albedo[center + 1]);
    assert!(albedo[center + 2] <= 2, "blue={}", albedo[center + 2]);
    assert_eq!(albedo[center + 3], 255);

    let orm = frame.readback(&mut device, FrameStage::Orm).unwrap();
    assert_eq!(orm[center], 255, "ao");
    assert!(
        (i32::from(orm[center + 1]) - quantize(0.4) as i32).abs() <= 2,
        "roughness={}",
        orm[center + 1]
    );
    assert!(orm[center + 2] <= 2, "metallic={}", orm[center + 2]);

    // Reversed-Z: центр куба ближе (0 < depth < 1), фон очищен в 0.
    let depth = frame.readback(&mut device, FrameStage::Depth).unwrap();
    let read = |offset: usize| {
        f32::from_bits(u32::from_le_bytes([
            depth[offset],
            depth[offset + 1],
            depth[offset + 2],
            depth[offset + 3],
        ]))
    };
    let center_depth = read(center);
    assert!(
        center_depth > 0.0 && center_depth < 1.0,
        "center depth={center_depth}"
    );
    assert_eq!(read(0), 0.0);

    device.wait_idle().unwrap();
    assert_eq!(
        device.validation_errors(),
        before,
        "validation log is dirty"
    );
    geometry.destroy(&mut device);
    frame.destroy(&mut device);
    device.wait_idle().unwrap();
    assert_eq!(device.live_resources(), 0);
}

#[test]
fn shadow_map_records_camera_relative_depth() {
    let Some(mut device) = runtime_device() else {
        return;
    };
    let before = device.validation_errors();
    let camera = Camera {
        aspect: 1.0,
        ..Camera::default()
    };
    let mut scene = RenderScene::new();
    scene.set_camera_position(camera.position);
    let position = Vec3::new(0.0, 0.0, -2.0);
    scene.push(position, position, 0, 0);
    scene.extract(
        1.0,
        &Frustum::from_view_projection(camera.view_projection()),
    );
    let mut geometry = SceneGeometry::new(&mut device, 4).unwrap();
    geometry.upload(&mut device, &scene).unwrap();

    let mut frame =
        GpuFrame::with_features(&mut device, SIZE, SIZE, FrameFeatures::none()).unwrap();
    frame
        .render_scene(&mut device, &FrameParams::default(), &geometry, &camera)
        .unwrap();

    let read_depth = |bytes: &[u8], x: u32, y: u32| {
        let offset = ((y * SHADOW_MAP_SIZE + x) * 4) as usize;
        f32::from_bits(u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]))
    };
    // Ортопроекция света центрирована на камере (origin), а куб стоит в
    // camera-relative (0, 0, -2): центр карты — точка под камерой, не куб.
    // Проверяем пиксель проекции центра куба; Y-flip между HLSL clip space
    // и Vulkan-framebuffer заранее неизвестен, поэтому берём оба варианта.
    let center_ndc = shadow_view_projection(SHADOW_LIGHT_DIR).project_point3(position);
    let pixel =
        |ndc: f32| (((ndc * 0.5 + 0.5) * SHADOW_MAP_SIZE as f32) as u32).min(SHADOW_MAP_SIZE - 1);
    let (px, py) = (pixel(center_ndc.x), pixel(center_ndc.y));
    let flipped_y = SHADOW_MAP_SIZE - 1 - py;
    let shadow = frame.readback(&mut device, FrameStage::Shadow).unwrap();
    assert_eq!(
        shadow.len(),
        (SHADOW_MAP_SIZE * SHADOW_MAP_SIZE * 4) as usize
    );
    let cube_depth = read_depth(&shadow, px, py).max(read_depth(&shadow, px, flipped_y));
    let filled = shadow
        .chunks_exact(4)
        .filter(|pixel| f32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]) > 0.0)
        .count();
    let max_depth = shadow
        .chunks_exact(4)
        .map(|pixel| f32::from_bits(u32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]])))
        .fold(0.0f32, f32::max);
    assert!(
        cube_depth > 0.0 && cube_depth < 1.0,
        "cube shadow depth={cube_depth} at ({px}, {py}) filled={filled} max={max_depth}"
    );
    assert!(filled > 0, "shadow map is empty");
    assert_eq!(read_depth(&shadow, 1, 1), 0.0, "вне куба — clear 0.0");

    // Camera-relative: камера и инстанс сдвинуты на 1e6, относительная
    // позиция та же — карта теней совпадает.
    let far_camera = Camera {
        position: Vec3::new(1.0e6, 0.0, 0.0),
        aspect: 1.0,
        ..Camera::default()
    };
    let mut far_scene = RenderScene::new();
    far_scene.set_camera_position(far_camera.position);
    let world = Vec3::new(1.0e6, 0.0, -2.0);
    far_scene.push(world, world, 0, 0);
    far_scene.extract(
        1.0,
        &Frustum::from_view_projection(far_camera.view_projection()),
    );
    geometry.upload(&mut device, &far_scene).unwrap();
    frame
        .render_scene(&mut device, &FrameParams::default(), &geometry, &far_camera)
        .unwrap();
    let far_shadow = frame.readback(&mut device, FrameStage::Shadow).unwrap();
    let center_far = read_depth(&far_shadow, px, py).max(read_depth(&far_shadow, px, flipped_y));
    assert!(
        (cube_depth - center_far).abs() < 1.0e-4,
        "camera-relative: {cube_depth} vs {center_far}"
    );
    assert_eq!(
        far_shadow, shadow,
        "camera-relative: карта теней не зависит от мировых координат"
    );

    device.wait_idle().unwrap();
    assert_eq!(
        device.validation_errors(),
        before,
        "validation log is dirty"
    );
    geometry.destroy(&mut device);
    frame.destroy(&mut device);
    device.wait_idle().unwrap();
    assert_eq!(device.live_resources(), 0);
}
