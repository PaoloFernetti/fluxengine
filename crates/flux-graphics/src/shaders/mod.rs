//! Шейдерные пермутации (ADR-0014, ADR-0036): декларации `shader.json`,
//! keyword-маски, off-demand компиляция с fallback и трёхуровневый кэш.
//!
//! Runtime-ядро не компилирует HLSL: оно принимает [`ShaderCompiler`]
//! (DXC-пайплайн — I6b) и гарантирует, что неизвестная комбинация keywords
//! даёт fallback-байткод, а не крэш. Порядок обхода — `BTreeMap`, поэтому
//! ключи и статистика детерминированы.

mod declaration;
mod system;

pub use declaration::{
    BindingDecl, KeywordDecl, KeywordSet, PipelineVariant, ShaderDeclaration, ShaderStage,
    VertexInput, MAX_KEYWORDS, MAX_MATERIAL_KEYWORDS, VERTEX_FORMAT_V1,
};
pub use system::{
    CacheStats, DeterministicCompiler, KeywordMask, PermutationCache, PermutationRequest,
    ResolvedShader, ShaderCompiler, ShaderLibrary,
};

use thiserror::Error;

/// Ошибки деклараций и пермутаций.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ShaderError {
    /// Некорректная декларация `shader.json`.
    #[error("некорректная декларация шейдера: {0}")]
    InvalidDeclaration(String),
    /// Шейдер не зарегистрирован в библиотеке.
    #[error("шейдер {0:#x} не зарегистрирован")]
    UnknownShader(u128),
    /// Keyword отсутствует в декларации или относится к другому набору.
    #[error("неизвестный keyword: {0}")]
    UnknownKeyword(String),
    /// Fallback ещё не прекомпилирован.
    #[error("нет fallback-шейдера для {0:#x}")]
    NoFallback(u128),
    /// Компилятор шейдеров вернул ошибку.
    #[error("компилятор шейдеров: {0}")]
    CompileFailed(String),
    /// Превышен бюджет живых пермутаций (красный флаг в dev-логе).
    #[error("превышен бюджет пермутаций: {live} из {max}")]
    PermutationBudgetExceeded {
        /// Сколько пермутаций уже живёт.
        live: usize,
        /// Настроенный лимит.
        max: usize,
    },
}
