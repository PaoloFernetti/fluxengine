//! M3: окно winit + softbuffer + детерминированная симуляция.
//!
//! Каждый `RedrawRequested` двигает [`DeterminismHarness`] на один тик,
//! рисует кадр в softbuffer и обновляет заголовок окна тиком и хешем
//! состояния. Движок поставляет телеметрию через dev-overlay (ADR-0033).
//!
//! Запуск: `cargo run -p flux-engine --example m3`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::num::NonZeroU32;
use std::sync::Arc;

use flux_engine::{DeterminismHarness, EngineBuilder};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// Приложение M3.
struct App {
    window: Option<Arc<Window>>,
    context: Option<softbuffer::Context<Arc<Window>>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    engine: flux_engine::Engine,
    harness: DeterminismHarness,
}

impl App {
    fn new() -> Self {
        let engine = EngineBuilder::new().with_worker_threads(2).build().unwrap();
        let mut app = Self {
            window: None,
            context: None,
            surface: None,
            engine,
            harness: DeterminismHarness::new(0xF10C_5EED),
        };
        app.engine.set_entity_count(Some(1));
        app
    }

    fn redraw(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let Some(surface) = self.surface.as_mut() else {
            return;
        };

        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };

        self.harness.step();
        let state = self.harness.state_hash();
        let _ = self.engine.frame();

        surface.resize(width, height).unwrap();
        let mut buffer = surface.buffer_mut().unwrap();

        // Фон: тёмно-синий. Прямоугольник сдвигается по тику детерминированно.
        for pixel in buffer.iter_mut() {
            *pixel = 0x0010_1A2C;
        }

        let tick = self.harness.tick().get() as u32;
        let rect_w = (width.get() / 8).max(1);
        let rect_h = (height.get() / 8).max(1);
        let x0 = (tick % width.get().saturating_sub(rect_w).max(1)).min(width.get() - 1);
        let y0 = (height.get() / 2).saturating_sub(rect_h / 2);

        for y in y0..(y0 + rect_h).min(height.get()) {
            let row = (y * width.get()) as usize;
            for x in x0..(x0 + rect_w).min(width.get()) {
                buffer[row + x as usize] = 0x004C_9AFF;
            }
        }

        buffer.present().unwrap();

        let overlay = self.engine.overlay();
        window.set_title(&format!(
            "FluxEngine M3 tick={} hash={:016x} | {}",
            self.harness.tick().get(),
            state.hash,
            overlay.summary(),
        ));
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("FluxEngine M3"))
                .unwrap(),
        );
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        self.context = Some(context);
        self.surface = Some(surface);
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state.is_pressed()
                    && event.logical_key == Key::Named(NamedKey::Escape) =>
            {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let (Some(surface), Some(width), Some(height)) = (
                    self.surface.as_mut(),
                    NonZeroU32::new(size.width),
                    NonZeroU32::new(size.height),
                ) {
                    surface.resize(width, height).unwrap();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
