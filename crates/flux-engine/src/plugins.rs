//! Plugin-система: подсистемы и геймплей регистрируются одинаково.
//!
//! Плагин получает [`EngineContext`] и работает только через контракты.
//! Порядок вызовов внутри кадра:
//! `build` (однократно) → `update_fixed` (0..N раз за `tick`) →
//! `update_variable` (раз за `frame`) → `shutdown`.

use crate::context::EngineContext;

/// Плагин движка.
///
/// `build` вызывается один раз при сборке [`crate::Engine`]; `&self` —
/// по контракту ADR-0027: регистрация ресурсов идёт через контекст, а не
/// через мутацию плагина. Мутабельные шаги получают `&mut self`.
pub trait Plugin: Send + Sync {
    /// Имя плагина для логов и диагностики.
    fn name(&self) -> &str;

    /// Регистрирует ресурсы и подписки. Вызывается до первого кадра.
    fn build(&self, ctx: &mut EngineContext);

    /// Фиксированный шаг симуляции. Может выполниться несколько раз за кадр.
    fn update_fixed(&mut self, _ctx: &mut EngineContext, _step: u32) {}

    /// Переменный шаг кадра: UI, стриминг, рендер-подготовка.
    fn update_variable(&mut self, _ctx: &mut EngineContext) {}

    /// Освобождает ресурсы перед остановкой движка.
    fn shutdown(&mut self) {}
}
