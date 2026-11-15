# flux-assetc

Компилятор ассетов: KTX2, меши, Opus, .fpf, шейдеры.

Канон: [docs/canon.md](../../docs/canon.md). План: [docs/architecture.md](../../docs/architecture.md).

## Шейдеры (I6b, ADR-0035/ADR-0036)

Офлайн-компиляция HLSL → SPIR-V через DXC:

```text
flux-assetc shader pbr.frag.hlsl --out out/pbr.frag.spv \
  --entry fs_main --stage ps_6_0 \
  --define FLUX=1 --include shaders/include
```

DXC ищется в `DXC_PATH`, рядом с `flux-assetc`, в `tools/dxc/bin`, затем в
`PATH` (`dxc.exe`/`dxc`). Отсутствие компилятора — типизированная ошибка,
а не паника:

```text
flux-assetc shader --check
```

Пути `.cmd`/`.bat` на Windows запускаются через `cmd /C`, что позволяет
использовать скрипты-обёртки. Runtime-ядро пермутаций (`flux-graphics::shaders`)
получает уже готовый SPIR-V и не зависит от DXC.

DXC распространяется под LLVM Release License (NCSA) и разрешён как
build-time зависимость; `dxil.dll` берётся только из официального SDK/NuGet.
Бинарники не коммитятся — см. [tools/dxc/README.md](../dxc/README.md) и
ADR-0038.

## Импорт `.unitypackage` (ADR-0043)

```text
flux-assetc unitypackage pack.unitypackage --out imported \
  [--no-meta] [--overwrite] [--dry-run]
```

Пакет — gzip-сжатый tar с каталогами GUID: `pathname` (исходный путь),
`asset` (содержимое), `asset.meta` (Unity-meta), опционально `preview.png`.
Инструмент распаковывает архив во временный staging, читает `pathname` и
восстанавливает дерево:

```text
imported/Assets/Textures/rock.png
imported/Assets/Textures/rock.png.meta
imported/Assets/Textures/Folder/        # folderAsset: yes или путь с "/"
```

- `--no-meta` — не писать `<asset>.meta`;
- `--overwrite` — перезаписывать существующие файлы (по умолчанию пропуск);
- `--dry-run` — только показать пути и итог, `--out` не меняется.

`pathname` считается недоверенным: абсолютные пути, `..`, `.`, префиксы
дисков и пустые компоненты отвергаются (`UnsafePath`), выход за пределы
`--out` невозможен. GUID-каталоги обходятся в сортированном порядке, поэтому
результат детерминирован. `preview.png` не извлекается, форматы Unity
(`.prefab`, `.mat`) не конвертируются — это задача контент-пайплайна 5.1.
