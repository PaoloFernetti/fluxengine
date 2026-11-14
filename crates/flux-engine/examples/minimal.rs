//! Мини-срез «движок дышит» (WP 1.6, M1).
//!
//! - окно winit, poll events, clear-color через softbuffer;
//! - два параллельных job на каждом фиксированном шаге (лог потоков);
//! - на 120-м кадре — намеренная паника в job: лог + breadcrumbs, процесс жив;
//! - dev-overlay: FPS, P50/P99 frametime, число воркеров и завершённых job.
//!
//! Запуск: `cargo run -p flux-engine --example minimal`.
//! Выход: Escape или закрытие окна.

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use flux_core::crash::breadcrumbs;
use flux_engine::bootstrap::{EngineBuilder, EngineConfig};
use flux_engine::context::EngineContext;
use flux_engine::plugins::Plugin;
use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// Плагин-демо: два параллельных job на шаг и одна паника на 120-м кадре.
struct DemoPlugin {
    jobs_done: Arc<AtomicU64>,
    frame: u64,
    panicked_once: bool,
}

impl DemoPlugin {
    fn new(jobs_done: Arc<AtomicU64>) -> Self {
        Self {
            jobs_done,
            frame: 0,
            panicked_once: false,
        }
    }
}

impl Plugin for DemoPlugin {
    fn name(&self) -> &str {
        "demo"
    }

    fn build(&self, _ctx: &mut EngineContext) {
        tracing::info!("demo plugin built");
    }

    fn update_fixed(&mut self, ctx: &mut EngineContext, _step: u32) {
        for job_id in 0..2_u32 {
            let counter = Arc::clone(&self.jobs_done);
            ctx.spawn(move || {
                breadcrumbs().push(format!("demo job {job_id}"));
                tracing::info!(
                    thread = ?std::thread::current().name(),
                    job = job_id,
                    "demo job ran"
                );
                counter.fetch_add(1, Ordering::Relaxed);
            });
        }
    }

    fn update_variable(&mut self, ctx: &mut EngineContext) {
        self.frame = self.frame.saturating_add(1);
        if self.frame == 120 && !self.panicked_once {
            self.panicked_once = true;
            ctx.spawn(|| panic!("intentional job panic (demo, WP 1.6)"));
        }
    }

    fn shutdown(&mut self) {
        tracing::info!(
            jobs_done = self.jobs_done.load(Ordering::Relaxed),
            "demo plugin shutdown"
        );
    }
}

struct App {
    engine: flux_engine::Engine,
    jobs_done: Arc<AtomicU64>,
    window: Option<Arc<Window>>,
    _context: Option<Context<Arc<Window>>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
}

impl App {
    fn render(&mut self) {
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

        if let Err(error) = surface.resize(width, height) {
            eprintln!("surface resize failed: {error}");
            return;
        }

        let Ok(mut buffer) = surface.buffer_mut() else {
            eprintln!("surface buffer unavailable");
            return;
        };

        // Медленно пульсирующий clear-color: видно, что кадры идут.
        let phase = (self.engine.frame_index() as f32 * 0.02).sin() * 0.5 + 0.5;
        let red = (24.0 + 40.0 * phase) as u32;
        let green = 28;
        let blue = (48.0 + 72.0 * (1.0 - phase)) as u32;
        buffer.fill((red << 16) | (green << 8) | blue);

        if let Err(error) = buffer.present() {
            eprintln!("present failed: {error}");
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("FluxEngine — minimal")
            .with_inner_size(LogicalSize::new(960.0, 540.0));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("failed to create window: {error}");
                event_loop.exit();
                return;
            }
        };

        let context = match Context::new(Arc::clone(&window)) {
            Ok(context) => context,
            Err(error) => {
                eprintln!("failed to create softbuffer context: {error}");
                event_loop.exit();
                return;
            }
        };

        let surface = match Surface::new(&context, Arc::clone(&window)) {
            Ok(surface) => surface,
            Err(error) => {
                eprintln!("failed to create softbuffer surface: {error}");
                event_loop.exit();
                return;
            }
        };

        self.window = Some(window);
        self._context = Some(context);
        self.surface = Some(surface);
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::Escape)
                {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let _ = self.engine.tick();
        let stats = self.engine.frame();

        if let Some(window) = &self.window {
            window.set_title(&format!(
                "FluxEngine — {:.0} FPS | p50 {:.2} ms | p99 {:.2} ms | jobs {} | panicked {} | workers {} | done {}",
                stats.fps,
                stats.p50_ms,
                stats.p99_ms,
                stats.jobs,
                stats.panicked_jobs,
                self.engine.jobs().worker_count(),
                self.jobs_done.load(Ordering::Relaxed),
            ));
            window.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.engine.shutdown();
        tracing::info!("event loop exited");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let jobs_done = Arc::new(AtomicU64::new(0));
    let engine = EngineBuilder::new()
        .with_config(EngineConfig {
            throttle: true,
            ..EngineConfig::default()
        })
        .with_crash_handler("target/crash-dumps")
        .with_plugin(DemoPlugin::new(Arc::clone(&jobs_done)))
        .build()?;

    let mut app = App {
        engine,
        jobs_done,
        window: None,
        _context: None,
        surface: None,
    };

    event_loop.run_app(&mut app)?;
    Ok(())
}
