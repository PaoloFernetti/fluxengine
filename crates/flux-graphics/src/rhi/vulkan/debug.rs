//! Validation layers: debug messenger и счётчики сообщений.

#![allow(unsafe_code)]

use std::ffi::CStr;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::vk;

static ERRORS: AtomicU64 = AtomicU64::new(0);
static WARNINGS: AtomicU64 = AtomicU64::new(0);

/// Сколько validation-ошибок пришло с момента старта процесса.
pub fn error_count() -> u64 {
    ERRORS.load(Ordering::Relaxed)
}

/// Сколько validation-предупреждений пришло с момента старта процесса.
#[allow(dead_code)]
pub fn warning_count() -> u64 {
    WARNINGS.load(Ordering::Relaxed)
}

unsafe extern "system" fn callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    let message = if data.is_null() {
        "vulkan validation message".to_owned()
    } else {
        // SAFETY: the callback contract guarantees `p_message` is NUL-terminated.
        let raw = unsafe { (*data).p_message };
        if raw.is_null() {
            "vulkan validation message".to_owned()
        } else {
            // SAFETY: non-null and NUL-terminated per the callback contract.
            unsafe { CStr::from_ptr(raw) }
                .to_string_lossy()
                .into_owned()
        }
    };
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        ERRORS.fetch_add(1, Ordering::Relaxed);
        eprintln!("[vulkan][validation][error] {message}");
    } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        WARNINGS.fetch_add(1, Ordering::Relaxed);
        eprintln!("[vulkan][validation][warn] {message}");
    }
    vk::FALSE
}

/// Debug messenger, живущий столько же, сколько instance.
pub(crate) struct DebugMessenger {
    ext: ash::ext::debug_utils::Instance,
    messenger: vk::DebugUtilsMessengerEXT,
}

impl DebugMessenger {
    /// Создаёт messenger, если расширение доступно.
    pub(crate) fn new(entry: &ash::Entry, instance: &ash::Instance) -> Option<Self> {
        let ext = ash::ext::debug_utils::Instance::new(entry, instance);
        let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(callback));
        let mut messenger = vk::DebugUtilsMessengerEXT::null();
        // SAFETY: `instance` is valid, the extension was enabled and the out
        // pointer is writable.
        let result = unsafe {
            (ext.fp().create_debug_utils_messenger_ext)(
                instance.handle(),
                &info,
                std::ptr::null(),
                &mut messenger,
            )
        };
        if result != vk::Result::SUCCESS {
            return None;
        }
        Some(Self { ext, messenger })
    }

    /// Уничтожает messenger. Вызывается до `destroy_instance`.
    pub(crate) fn destroy(self, instance: &ash::Instance) {
        // SAFETY: messenger was created by this extension and is destroyed once.
        unsafe {
            (self.ext.fp().destroy_debug_utils_messenger_ext)(
                instance.handle(),
                self.messenger,
                std::ptr::null(),
            );
        }
    }
}
