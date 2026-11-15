# DXC (вендорный каталог)

Каталог для локального/CI-размещения DirectX Shader Compiler. Бинарники **не
коммитятся** по политике (ADR-0038): добавьте `tools/dxc/bin/`, `tools/dxc/dxc`,
`tools/dxc/dxc.exe`, `LICENSE.TXT` и `ThirdPartyNotices.txt` в локальный или
корневой ignore; файлы кладутся из официального источника.

## Быстрая установка (локально и в CI)

По умолчанию [`fetch.mjs`](fetch.mjs) ставит **последний** релиз
DirectXShaderCompiler (GitHub API `releases/latest`) и обновляет установку при
выходе новой версии. Целостность — SHA-256 из `digest` ассета GitHub (если он
не опубликован, хэш сохраняется в `bin/.dxc-release.json`). Повторный запуск
идемпотентен:

```text
node tools/dxc/fetch.mjs                     # последний релиз
node tools/dxc/fetch.mjs --version v1.9.2609 # закрепить версию
node tools/dxc/fetch.mjs --force             # перекачать
```

- Windows: `tools/dxc/bin/{dxc.exe,dxcompiler.dll,dxil.dll,dxv.exe}`.
- Linux: `tools/dxc/bin/dxc-real`, `libdxcompiler.so`, `libdxil.so` и
  генерируемая обёртка `bin/dxc` с `LD_LIBRARY_PATH`.
- Лицензии поставки (`LICENSE-*.txt`, `LICENCE-*.txt`, `ThirdPartyNotices*.txt`)
  копируются в `tools/dxc/bin/licenses/` (весь `bin/` в ignore).

CI-шаг перед компиляцией шейдеров:

```yaml
- name: DXC
  run: node tools/dxc/fetch.mjs
```

Без DXC `flux-assetc shader` возвращает типизированную ошибку, поэтому
остальные шаги остаются зелёными.

## Лицензия и распространение

- DXC распространяется под **LLVM Release License** (University of
  Illinois/NCSA Open Source License) — пермиссивная BSD-подобная лицензия.
  Допускается использование, модификация и распространение, включая
  коммерческие и закрытые продукты, при сохранении текста лицензии и
  copyright-уведомлений.
- Отдельные компоненты могут иметь свои лицензии (см. `LICENSE.TXT` и
  `ThirdPartyNotices.txt` в поставке DXC) — их нужно сохранять при
  распространении.
- `dxil.dll` (валидатор/подписант DXIL) распространяется Microsoft в
  бинарном виде: брать только из официального Windows SDK / NuGet
  (`Microsoft.Direct3D.DXC`), не пересобирать.
- DXC — **build-time** инструмент: он не линкуется в движок и не влияет на
  лицензию рантайма (см. [legal.md](../../docs/legal.md), ADR-0038).

## Как положить

Официальные источники:

- NuGet: `Microsoft.Direct3D.DXC` (Windows/Linux);
- Windows SDK (каталог `bin/<arch>/`);
- Vulkan SDK.

Ожидаемая раскладка (любой из вариантов):

```text
tools/dxc/bin/dxc.exe        # Windows
tools/dxc/bin/dxc            # Linux (обёртка fetch.mjs или бинарник)
tools/dxc/dxc.exe            # плоская раскладка
```

Поиск: `DXC_PATH` → рядом с `flux-assetc` → `tools/dxc/bin` → `PATH`.
Проверка:

```text
flux-assetc shader --check
```

При распространении игры/движка приложите `LICENSE.TXT` и
`ThirdPartyNotices.txt` из поставки DXC.
