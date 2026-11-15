//! Surface/swapchain smoke на реальном Vulkan (I3/I4, ADR-0034).
//!
//! Использует `VK_EXT_headless_surface`, поэтому проверяет настоящие
//! `VkSurfaceKHR`/`VkSwapchainKHR`, acquire/present и resize без окна — в том
//! числе на lavapipe в CI. Если расширения нет (например, Windows-драйвер без
//! headless WSI), тест пропускается, кроме `FLUX_REQUIRE_VULKAN=1`, где
//! отсутствие поверхности считается ошибкой.

#![cfg(feature = "vulkan")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use flux_graphics::{
    BarrierBatch, BufferUsage, FrameFeatures, FrameParams, GpuBufferDesc, GpuFrame, GpuImageDesc,
    ImageId, ImageState, RenderPassDesc, RhiDevice, SurfaceTarget, SwapchainDesc, TextureFormat,
    TextureUsage, VulkanDevice,
};

fn runtime_device() -> Option<VulkanDevice> {
    match VulkanDevice::new() {
        Ok(device) => Some(device),
        Err(error) => {
            if std::env::var_os("FLUX_REQUIRE_VULKAN").is_some() {
                panic!("FLUX_REQUIRE_VULKAN is set, but Vulkan is unavailable: {error}");
            }
            eprintln!("skipping Vulkan surface test: {error}");
            None
        }
    }
}

#[test]
fn headless_surface_swapchain_presents_and_resizes() {
    let Some(mut device) = runtime_device() else {
        return;
    };
    let before = device.validation_errors();
    let surface = match device.create_surface(&SurfaceTarget::Headless) {
        Ok(surface) => surface,
        Err(error) => {
            if std::env::var_os("FLUX_REQUIRE_VULKAN").is_some() {
                panic!("FLUX_REQUIRE_VULKAN is set, but headless surface is unavailable: {error}");
            }
            eprintln!("skipping surface swapchain test: {error}");
            return;
        }
    };

    let desc = SwapchainDesc::new("headless", 64, 48)
        .with_format(TextureFormat::Bgra8Srgb)
        .with_surface(surface);
    let mut swapchain = device.create_swapchain(&desc).unwrap();
    assert!(swapchain.image_count() >= 1);
    assert!(!swapchain.format().is_depth());
    assert!(swapchain.size().0 > 0);

    for _ in 0..3 {
        let index = swapchain.acquire().unwrap();
        swapchain.present(index).unwrap();
    }

    // Resize 10× подряд: каждый раз цепочка пересоздаётся, число образов
    // остаётся стабильным (утечки образов/views не накапливаются).
    let images = swapchain.image_count();
    for step in 0..10u32 {
        swapchain.resize(64 + step, 48 + step).unwrap();
        assert_eq!(swapchain.image_count(), images);
        assert!(swapchain.size().0 > 0 && swapchain.size().1 > 0);
        let index = swapchain.acquire().unwrap();
        swapchain.present(index).unwrap();
    }

    assert_eq!(device.validation_errors(), before);
    drop(swapchain);
    assert!(device.destroy_surface(surface));
    assert!(!device.destroy_surface(surface));
}

#[test]
fn acquired_swapchain_image_is_a_render_target() {
    let Some(mut device) = runtime_device() else {
        return;
    };
    let before = device.validation_errors();
    let surface = match device.create_surface(&SurfaceTarget::Headless) {
        Ok(surface) => surface,
        Err(error) => {
            if std::env::var_os("FLUX_REQUIRE_VULKAN").is_some() {
                panic!("FLUX_REQUIRE_VULKAN is set, but headless surface is unavailable: {error}");
            }
            eprintln!("skipping swapchain image test: {error}");
            return;
        }
    };

    let desc = SwapchainDesc::new("headless-image", 64, 48)
        .with_format(TextureFormat::Bgra8Srgb)
        .with_surface(surface);
    let mut swapchain = device.create_swapchain(&desc).unwrap();
    let index = swapchain.acquire().unwrap();
    let image = swapchain.acquired_image().expect("acquired image");
    assert_eq!(swapchain.image(index), Some(image));
    // Образ принадлежит swapchain, устройство его не освобождает.
    assert!(!device.destroy_image(image));

    let mut commands = device.create_command_buffer().unwrap();
    commands.begin().unwrap();
    let pass = RenderPassDesc::new("present-clear")
        .with_color(image)
        .with_clear([0.0, 0.0, 1.0, 1.0]);
    commands.begin_render_pass(&pass).unwrap();
    commands.end_render_pass().unwrap();
    commands.end().unwrap();
    commands.finish().unwrap();
    device.wait_idle().unwrap();

    swapchain.present(index).unwrap();
    assert!(swapchain.acquired_image().is_none());
    assert_eq!(device.validation_errors(), before);

    drop(swapchain);
    assert!(device.destroy_surface(surface));
}

/// Читает образ в staging: барьер `from → TransferSrc`, копия, readback.
fn read_image(
    device: &mut VulkanDevice,
    image: ImageId,
    from: ImageState,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let bytes = u64::from(width) * u64::from(height) * 4;
    let staging = device
        .create_buffer(&GpuBufferDesc::new(
            "surface-readback",
            bytes,
            BufferUsage::staging(),
        ))
        .unwrap();
    let mut commands = device.create_command_buffer().unwrap();
    commands.begin().unwrap();
    let mut batch = BarrierBatch::new();
    batch.transition(image, from, ImageState::TransferSrc);
    commands.barriers(&batch).unwrap();
    commands.copy_image_to_buffer(image, staging).unwrap();
    commands.end().unwrap();
    commands.finish().unwrap();
    device.wait_idle().unwrap();
    let data = device.read_buffer(staging).unwrap();
    assert!(device.destroy_buffer(staging));
    data
}

#[test]
fn ui_stub_is_presented_to_swapchain() {
    let Some(mut device) = runtime_device() else {
        return;
    };
    let before = device.validation_errors();
    let surface = match device.create_surface(&SurfaceTarget::Headless) {
        Ok(surface) => surface,
        Err(error) => {
            if std::env::var_os("FLUX_REQUIRE_VULKAN").is_some() {
                panic!("FLUX_REQUIRE_VULKAN is set, but headless surface is unavailable: {error}");
            }
            eprintln!("skipping UI present test: {error}");
            return;
        }
    };

    let size = 32u32;
    let desc = SwapchainDesc::new("ui-present", size, size)
        .with_format(TextureFormat::Bgra8Srgb)
        .with_surface(surface);
    let mut swapchain = device.create_swapchain(&desc).unwrap();
    let mut frame =
        GpuFrame::with_features(&mut device, size, size, FrameFeatures::none()).unwrap();
    let params = FrameParams::default();

    // Эталон: тот же кадр без UI в offscreen-образ того же формата.
    let reference = device
        .create_image(
            &GpuImageDesc::new("ui-reference", size, size, TextureFormat::Bgra8Srgb).with_usage(
                TextureUsage {
                    render_target: true,
                    transfer_src: true,
                    ..TextureUsage::none()
                },
            ),
        )
        .unwrap();
    let mut commands = device.create_command_buffer().unwrap();
    commands.begin().unwrap();
    let mut batch = BarrierBatch::new();
    batch.transition(
        reference,
        ImageState::Undefined,
        ImageState::ColorAttachment,
    );
    commands.barriers(&batch).unwrap();
    commands.end().unwrap();
    commands.finish().unwrap();
    frame
        .render_to(&mut device, &params, reference, TextureFormat::Bgra8Srgb)
        .unwrap();
    let reference_bytes = read_image(
        &mut device,
        reference,
        ImageState::ColorAttachment,
        size,
        size,
    );

    let index = frame
        .present(&mut device, &params, &mut *swapchain)
        .unwrap();
    let image = swapchain.image(index).expect("presented image");
    let presented = read_image(&mut device, image, ImageState::Present, size, size);

    let pixel = |bytes: &[u8], x: u32, y: u32| {
        let offset = ((y * size + x) * 4) as usize;
        [
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]
    };
    // Панель UI-stub затемняет левый верхний угол.
    let panel_reference = pixel(&reference_bytes, 2, 2);
    let panel_ui = pixel(&presented, 2, 2);
    assert!(
        panel_ui[0] + 10 < panel_reference[0],
        "UI panel not applied: reference={panel_reference:?} presented={panel_ui:?}"
    );
    // Вне панели кадры совпадают: TAA выключен, рендер детерминирован.
    let outside_reference = pixel(&reference_bytes, size - 2, size - 2);
    let outside_ui = pixel(&presented, size - 2, size - 2);
    assert_eq!(outside_reference, outside_ui);

    device.wait_idle().unwrap();
    assert_eq!(
        device.validation_errors(),
        before,
        "validation log is dirty"
    );
    frame.destroy(&mut device);
    assert!(device.destroy_image(reference));
    drop(swapchain);
    assert!(device.destroy_surface(surface));
    device.wait_idle().unwrap();
    assert_eq!(device.live_resources(), 0);
}
