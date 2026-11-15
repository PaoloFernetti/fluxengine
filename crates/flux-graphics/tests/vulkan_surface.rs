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
    RenderPassDesc, RhiDevice, SurfaceTarget, SwapchainDesc, TextureFormat, VulkanDevice,
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
