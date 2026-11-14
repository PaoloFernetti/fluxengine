//! Ядро FluxEngine: время, память, события, job system, логи, краши, конфиг.
//!
//! Ядро не знает о подсистемах: только контракты и базовые сервисы.
//! Канон: `docs/canon.md`. План: `docs/architecture.md`, раздел «Детальный дизайн ядра».

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod config;
pub mod contracts;
pub mod crash;
pub mod events;
pub mod input;
pub mod jobs;
pub mod logging;
pub mod memory;
pub mod platform;
pub mod schema;
pub mod time;

pub use flux_math;

pub use config::{ConfigError, ConfigSection, ConfigStore};
pub use crash::{breadcrumbs, Breadcrumbs, CrashHandler};
pub use events::{EventRegistry, Reader, Writer};
pub use input::Intention;
pub use jobs::{JobHandle, JobStatus, JobSystem, MainThreadTaskQueue};
pub use logging::{init_logging, FrameTelemetry};
pub use memory::{FrameArena, GenerationalIndex, MemoryTag, OomPolicy, PoolAllocator, SlotMap};
pub use schema::{ComponentLayout, FieldDescriptor, FieldFlags, FieldId, SchemaRef, TypeId};
pub use time::{Clock, FixedTimestep, SimTick, SimTime, Stopwatch, TimeContext, TimeUpdate};
