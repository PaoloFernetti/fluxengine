//! Интеграционный PSO-смоук на реальном Vulkan (I5, ADR-0035).
//!
//! Проверяет, что закоммиченный DXC-артефакт SPIR-V принимается драйвером,
//! графический pipeline собирается, а кадр с `draw` проходит без
//! validation-ошибок. Без доступного Vulkan-устройства тест завершается
//! успешно (headless-CI не требует GPU).

#![cfg(feature = "vulkan")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use flux_graphics::{
    BarrierBatch, BindingKind, BufferUsage, DescriptorBinding, DescriptorSetDesc, DescriptorWrite,
    GpuBufferDesc, GpuImageDesc, ImageState, PipelineCacheFile, PipelineCacheLoad, PipelineDesc,
    RenderPassDesc, RhiDevice, TextureFormat, TextureUsage, VulkanDevice,
};

const FULLSCREEN_VS: &[u8] = include_bytes!("../shaders/compiled/fullscreen.vert.spv");
const SOLID_FS: &[u8] = include_bytes!("../shaders/compiled/solid.frag.spv");

const COLOR_FORMATS: &[TextureFormat] = &[TextureFormat::Rgba8Srgb];

fn runtime_device() -> Option<VulkanDevice> {
    match VulkanDevice::new() {
        Ok(device) => Some(device),
        Err(error) => {
            // На GPU-раннере отсутствие устройства — ошибка, а не skip:
            // иначе lavapipe-job зелёный, ничего не проверив.
            if std::env::var_os("FLUX_REQUIRE_VULKAN").is_some() {
                panic!("FLUX_REQUIRE_VULKAN is set, but Vulkan is unavailable: {error}");
            }
            eprintln!("skipping Vulkan test: {error}");
            None
        }
    }
}

fn spirv_magic(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[test]
fn committed_spirv_is_vulkan_module() {
    for (label, bytes) in [("fullscreen.vert", FULLSCREEN_VS), ("solid.frag", SOLID_FS)] {
        assert!(bytes.len() >= 20, "{label}: SPIR-V слишком короткий");
        assert_eq!(bytes.len() % 4, 0, "{label}: SPIR-V не кратен 4 байтам");
        assert_eq!(
            spirv_magic(bytes),
            0x0723_0203,
            "{label}: неверная сигнатура SPIR-V"
        );
    }
}

#[test]
fn solid_pipeline_draws_without_validation_errors() {
    let Some(mut device) = runtime_device() else {
        return;
    };
    let before = device.validation_errors();

    let target = device
        .create_image(
            &GpuImageDesc::new("pso-target", 64, 64, TextureFormat::Rgba8Srgb)
                .with_usage(TextureUsage::render_target()),
        )
        .unwrap();

    let desc = PipelineDesc::graphics("solid", FULLSCREEN_VS, SOLID_FS)
        .with_color_formats(COLOR_FORMATS)
        // Полноэкранный треугольник строится из SV_VertexID: vertex buffer нет.
        .with_vertex_stride(0);
    let pipeline = device.create_pipeline(&desc).unwrap();
    assert_eq!(device.pipeline_count(), 1);

    let mut commands = device.create_command_buffer().unwrap();
    commands.begin().unwrap();
    let mut batch = BarrierBatch::new();
    batch.transition(target, ImageState::Undefined, ImageState::ColorAttachment);
    commands.barriers(&batch).unwrap();

    let pass = RenderPassDesc::new("pso")
        .with_color(target)
        .with_clear([0.0, 0.0, 0.0, 1.0]);
    commands.begin_render_pass(&pass).unwrap();
    commands.set_pipeline(pipeline).unwrap();
    commands.draw(3, 1).unwrap();
    commands.end_render_pass().unwrap();

    let mut batch = BarrierBatch::new();
    batch.transition(target, ImageState::ColorAttachment, ImageState::TransferSrc);
    commands.barriers(&batch).unwrap();
    commands.end().unwrap();
    commands.finish().unwrap();
    device.wait_idle().unwrap();

    assert_eq!(device.validation_errors(), before);
    assert!(device.destroy_pipeline(pipeline));
    assert_eq!(device.pipeline_count(), 0);
    assert!(device.destroy_image(target));
}

#[test]
fn pipeline_cache_survives_device_restart() {
    let Some(mut device) = runtime_device() else {
        return;
    };
    let desc = PipelineDesc::graphics("solid", FULLSCREEN_VS, SOLID_FS)
        .with_color_formats(COLOR_FORMATS)
        .with_vertex_stride(0);
    let pipeline = device.create_pipeline(&desc).unwrap();
    device.wait_idle().unwrap();
    let blob = device.pipeline_cache().unwrap();
    assert!(
        !blob.is_empty(),
        "драйвер вернул пустой блоб VkPipelineCache после создания PSO"
    );
    let signature = device.device_signature();
    let path = std::env::temp_dir().join(format!("flux-pso-smoke-{}.bin", std::process::id()));
    let file = PipelineCacheFile::new(&path, signature.clone());
    file.save(&blob).unwrap();
    assert!(matches!(
        file.load().unwrap(),
        PipelineCacheLoad::Loaded(ref data) if data == &blob
    ));
    assert!(device.destroy_pipeline(pipeline));
    drop(device);

    // Второй «запуск»: новое устройство загружает блоб с диска и создаёт
    // тот же PSO. Это проверяет формат и инвалидацию, а не счётчик
    // компиляций драйвера (он недоступен через Vulkan API).
    let Some(mut second) = runtime_device() else {
        return;
    };
    assert_eq!(second.device_signature(), signature);
    let PipelineCacheLoad::Loaded(loaded) = file.load().unwrap() else {
        panic!("кэш PSO пропал между запусками");
    };
    assert_eq!(loaded, blob);
    second.load_pipeline_cache(&loaded).unwrap();
    let again = second.create_pipeline(&desc).unwrap();
    second.wait_idle().unwrap();
    assert!(!second.pipeline_cache().unwrap().is_empty());
    assert!(second.destroy_pipeline(again));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn draw_requires_bound_pipeline() {
    let Some(mut device) = runtime_device() else {
        return;
    };
    let mut commands = device.create_command_buffer().unwrap();
    commands.begin().unwrap();
    assert!(commands.draw(3, 1).is_err());
    commands.end().unwrap();
    commands.finish().unwrap();
}

#[test]
fn buffer_roundtrip_and_descriptor_set() {
    let Some(mut device) = runtime_device() else {
        return;
    };
    let before = device.validation_errors();

    let staging = device
        .create_buffer(&GpuBufferDesc::new("staging", 64, BufferUsage::staging()))
        .unwrap();
    device.write_buffer(staging, 0, &[1, 2, 3, 4]).unwrap();
    let bytes = device.read_buffer(staging).unwrap();
    assert_eq!(bytes.len(), 64);
    assert_eq!(&bytes[..4], &[1, 2, 3, 4]);
    assert!(device.write_buffer(staging, 62, &[1, 2, 3, 4]).is_err());

    let storage = device
        .create_buffer(&GpuBufferDesc::new("storage", 64, BufferUsage::storage()))
        .unwrap();
    let texture = device
        .create_image(&GpuImageDesc::new("tex", 4, 4, TextureFormat::Rgba8Srgb))
        .unwrap();

    let bindings = [
        DescriptorBinding::new(0, BindingKind::StorageBuffer),
        DescriptorBinding::new(1, BindingKind::SampledImage).with_count(2),
    ];
    let set = device
        .create_descriptor_set(&DescriptorSetDesc::new("set0").with_bindings(&bindings))
        .unwrap();
    device
        .update_descriptor_set(
            set,
            &[
                DescriptorWrite::buffer(0, 0, storage),
                DescriptorWrite::image(1, 1, texture),
            ],
        )
        .unwrap();
    assert!(device
        .update_descriptor_set(set, &[DescriptorWrite::image(0, 0, texture)])
        .is_err());
    assert!(device
        .update_descriptor_set(set, &[DescriptorWrite::image(1, 2, texture)])
        .is_err());

    device.wait_idle().unwrap();
    assert_eq!(device.validation_errors(), before);
    assert!(device.destroy_descriptor_set(set));
    assert!(!device.destroy_descriptor_set(set));
    assert!(device.destroy_image(texture));
    assert!(device.destroy_buffer(storage));
    assert!(device.destroy_buffer(staging));
}

#[test]
fn clear_frame_reads_back_through_staging_buffer() {
    let Some(mut device) = runtime_device() else {
        return;
    };
    let before = device.validation_errors();

    let usage = TextureUsage {
        render_target: true,
        transfer_src: true,
        ..TextureUsage::none()
    };
    let target = device
        .create_image(
            &GpuImageDesc::new("readback-target", 4, 4, TextureFormat::Rgba8Srgb).with_usage(usage),
        )
        .unwrap();
    let staging = device
        .create_buffer(&GpuBufferDesc::new("readback", 64, BufferUsage::staging()))
        .unwrap();
    let storage = device
        .create_buffer(&GpuBufferDesc::new("storage", 64, BufferUsage::storage()))
        .unwrap();

    let mut commands = device.create_command_buffer().unwrap();
    commands.begin().unwrap();
    let mut batch = BarrierBatch::new();
    batch.transition(target, ImageState::Undefined, ImageState::ColorAttachment);
    commands.barriers(&batch).unwrap();
    let pass = RenderPassDesc::new("clear")
        .with_color(target)
        .with_clear([1.0, 0.5, 0.25, 1.0]);
    commands.begin_render_pass(&pass).unwrap();
    commands.end_render_pass().unwrap();
    let mut batch = BarrierBatch::new();
    batch.transition(target, ImageState::ColorAttachment, ImageState::TransferSrc);
    commands.barriers(&batch).unwrap();
    // Readback обязан идти в staging-буфер (ADR-0039).
    assert!(commands.copy_image_to_buffer(target, storage).is_err());
    commands.copy_image_to_buffer(target, staging).unwrap();
    commands.end().unwrap();
    commands.finish().unwrap();
    device.wait_idle().unwrap();

    let pixels = device.read_buffer(staging).unwrap();
    assert_eq!(pixels.len(), 64);
    assert!(
        pixels.iter().any(|byte| *byte != 0),
        "clear color did not reach the readback buffer"
    );
    assert_eq!(pixels[3], 255);
    assert_eq!(device.validation_errors(), before);
    assert!(device.destroy_buffer(storage));
    assert!(device.destroy_buffer(staging));
    assert!(device.destroy_image(target));
}
