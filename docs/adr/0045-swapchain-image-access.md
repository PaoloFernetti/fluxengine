# 0045. Образы swapchain как RHI-ресурсы: рендер и present из Render Graph

- Статус: Accepted
- Дата: 2026-11-15
- Затрагивает: flux-graphics
- Supersedes: —

## Контекст

До сих пор `Swapchain` отдавал только `acquire() -> u32` и `present(index)`.
Образ цепочки нельзя было указать цветовым аттачментом, прочитать или
скопировать: Render Graph работал только с offscreen-изображениями, а
финальный `present` из графа был невозможен. Пункт 4.5 Roadmap требует
пайплайн «… → UI-stub → present», и это упирается именно в отсутствие
контракта доступа к образу.

`ImageState::Present` уже существовал, но `acquire` сразу переводил образ в
`PRESENT_SRC_KHR` (содержимое не нужно) — рендерить в него было нельзя.

## Варианты

1. **Отдельный тип `SwapchainImage` + команды копирования в него.** Не даёт
   рендерить напрямую, требует нового канала команд и дублирует регистрацию
   образов в бэкендах.
2. **Владелец образа — вызывающий (`create_image` из swapchain).** Ломает
   владение `VkSwapchainKHR` и `VkImage`: образы цепочки нельзя уничтожать
   по одному.
3. **Образы swapchain регистрируются в реестре устройства как `ImageId`**
   (выбрано): тот же путь, что у обычных изображений, с флагом владения.

## Решение

`Swapchain` расширен:

```rust
fn image(&self, index: u32) -> Option<ImageId>;
fn acquired_image(&self) -> Option<ImageId>;
```

- Образы цепочки регистрируются в реестре устройства при создании swapchain
  (Vulkan: `ImageEntry { owned: false, allocation: None }`; Null: отдельный
  общий реестр образов swapchain). Хэндл валиден до `resize`/уничтожения
  цепочки.
- `RhiDevice::destroy_image` для такого образа возвращает `false` — владелец
  swapchain; освобождение происходит при пересоздании/`Drop`.
- `acquire` переводит образ `Undefined → ColorAttachment`; Render Graph может
  использовать его как внешнее изображение (`GraphImage::external`) и писать
  в него напрямую.
- `present(index)` переводит `ColorAttachment → Present` и вызывает
  `queue_present`. Ожидается, что содержимое записано как в цветовой
  аттачмент; иное состояние вызывающий обязан привести к `ColorAttachment`
  сам.
- При пересоздании образов (resize/пересоздание swapchain) кэш
  `VkFramebuffer` инвалидируется: старые framebuffer-ы ссылаются на
  уничтоженные `VkImageView`.
- Null-бэкенд создаёт образы цепочки в общем реестре и освобождает их в
  `Drop`; `present` валидирует индекс и сбрасывает acquire.

## Следствия

- Render Graph и `present` используют один и тот же `ImageId`; отдельного
  «present-прохода» в RHI не требуется.
- Хэндлы образов недействительны после `resize` — это документировано и
  покрыто тестами (Null: clear → readback → present; Vulkan: clear в
  acquired-образ → present, чистый validation).
- Offscreen-Vulkan swapchain (`VkSwapchainKHR` без поверхности) следует тому же
  контракту: образы регистрируются, но освобождаются самой цепочкой.
- `ImageEntry` получил `owned`/`Option<Allocation>`; устройства, создающие
  образы, освобождают их как раньше, а образы цепочек — нет.
- `present` теперь выполняет переход layout-а (раньше это делал `acquire`);
  поведение offscreen-цепочки не меняется.
