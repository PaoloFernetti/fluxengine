# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.20.1] - 2026-11-15

### Fixes

#### ci

- релизный тег пушится PAT владельца ([850c3a4](https://github.com/PaoloFernetti/fluxengine/commit/850c3a46da0d57614aa9cdc0e1132a6fb8c435f6))

## [0.20.0] - 2026-11-15

### Features

#### graphics

- процедурное небо и аналитический IBL (ADR-0051) ([d1e334f](https://github.com/PaoloFernetti/fluxengine/commit/d1e334f155cf86e67fe1e936c948aacd39f8228e))

## [0.19.0] - 2026-11-15

### Features

#### graphics

- shadow map с camera-relative light space (ADR-0050) ([dc5bdb0](https://github.com/PaoloFernetti/fluxengine/commit/dc5bdb0e61ac40abbc1d065eccfa53e287abce58))

## [0.18.0] - 2026-11-15

### Features

#### graphics

- depth-only PSO и состояние глубины (ADR-0049) ([2acfe49](https://github.com/Kenift/fluxengine/commit/2acfe495c4d7d03384ddc6413010d8a564886b1f))

### Fixes

#### ci

- релиз на встроенном github.token ([be53f75](https://github.com/Kenift/fluxengine/commit/be53f75a29d6517c7e9061866472989222e960b6))

## [0.17.2] - 2026-11-15

### Fixes

#### graphics

- UNORM-формат вывода для headless WSI ([df95de0](https://github.com/PaoloFernetti/fluxengine/commit/df95de0e5487f7e1e1ed593437cc685b2aa292bd))

## [0.17.1] - 2026-11-15

### Fixes

#### graphics

- эталон UI-stub без frame.ui в Vulkan-тесте ([b37b324](https://github.com/PaoloFernetti/fluxengine/commit/b37b32447f9885e34998db1c1ea1edc03f92c57b))

## [0.17.0] - 2026-11-15

### Features

#### graphics

- UI-stub и present в образ swapchain (4.5, ADR-0048) ([fd7b913](https://github.com/PaoloFernetti/fluxengine/commit/fd7b91349e5f07b19ced36d1f1e4de38a29a2186))

## [0.16.0] - 2026-11-15

### Features

#### graphics

- GPU-геометрия сцены через vertex pulling (4.5, ADR-0047) ([082fa67](https://github.com/PaoloFernetti/fluxengine/commit/082fa67991efe3f446e4b28eefdc504a075147c0))

## [0.15.0] - 2026-11-15

### Features

#### graphics

- copy_buffer_to_buffer для upload в storage (ADR-0046) ([23d171e](https://github.com/PaoloFernetti/fluxengine/commit/23d171e0cb2991c93aa2cbb76e3351a7cbe67691))

### Fixes

#### ci

- GITHUB_TOKEN для шага fetch DXC ([b02814d](https://github.com/PaoloFernetti/fluxengine/commit/b02814d0276fe20a72fa187b1c2dada1f5b9677e))

## [0.14.0] - 2026-11-15

### Features

#### graphics

- образы swapchain как ImageId (4.5, ADR-0045) ([daca1e0](https://github.com/PaoloFernetti/fluxengine/commit/daca1e097ebeeaff7c70ec4e37dace36b017a500))

## [0.13.0] - 2026-11-15

### Features

#### dxc

- загрузка последнего релиза DXC (пин опционален) ([7033ea1](https://github.com/PaoloFernetti/fluxengine/commit/7033ea15f2ede0c8919e48cfdc7f0be45943068e))

## [0.12.0] - 2026-11-15

### Features

#### dxc

- команда загрузки вендорного DXC для локали и CI ([a1d3ad6](https://github.com/PaoloFernetti/fluxengine/commit/a1d3ad6bb8b6f77bcbe423de6a342575629f5434))

## [0.11.0] - 2026-11-15

### Features

#### graphics

- culling и бюджет extract (4.5) ([0db975a](https://github.com/PaoloFernetti/fluxengine/commit/0db975a5872a9eebdb4abc6c26c266db0d4801e9))

## [0.10.0] - 2026-11-15

### Features

#### graphics

- рантайм-применение GraphicsSettings (4.8) ([6ff015e](https://github.com/PaoloFernetti/fluxengine/commit/6ff015e4bd97ec86fb25dd0046282da23a0dfc8e))

## [0.9.0] - 2026-11-15

### Features

#### assetc

- импорт .unitypackage через flux-assetc ([e559750](https://github.com/PaoloFernetti/fluxengine/commit/e5597500b39cbf6792e7786e00b7aad267cce856))

## [0.8.0] - 2026-11-15

### Features

#### graphics

- FrameStats по пассам кадра через query pools ([1df00d8](https://github.com/PaoloFernetti/fluxengine/commit/1df00d8f9742525f9fe4987247ea729854fc10a7))

## [0.7.0] - 2026-11-14

### Features

#### graphics

- GPU-проходы кадра G-buffer → SSAO → lighting → bloom → tonemap → TAA ([30015c8](https://github.com/PaoloFernetti/fluxengine/commit/30015c898ec74dcf7c64d53057cad881767fe217))

## [0.6.1] - 2026-11-14

### Fixes

#### graphics

- Linux-сборка window surface (xlib/xcb/wayland) ([f58e924](https://github.com/PaoloFernetti/fluxengine/commit/f58e924e40eecab594ec90b4e26d8d716663a31f))

### Internal

#### ci

- аннотировать падения Vulkan-шагов и проверять headless WSI ([93dfa7a](https://github.com/PaoloFernetti/fluxengine/commit/93dfa7a093a0f0b52a07cf5071e88cbf8b80db8a))

## [0.6.0] - 2026-11-14

### Features

#### graphics

- поверхности VkSurfaceKHR и present-swapchain ([7778c44](https://github.com/PaoloFernetti/fluxengine/commit/7778c445bbc10d86fbc057e2130d029fc08aa6ed))

## [0.5.0] - 2026-11-14

### Features

#### graphics

- дисковый кэш PSO с инвалидацией по устройству ([1f496fe](https://github.com/PaoloFernetti/fluxengine/commit/1f496fe91cefbae5e3e38798a1e6ed5c6a00e644))

## [0.4.1] - 2026-11-14

### Fixes

#### graphics

- staging-контракт по usage-флагам на Vulkan ([70eed5b](https://github.com/PaoloFernetti/fluxengine/commit/70eed5b8d7a42185cab1ea67d009589db1339003))

### Documentation

#### roadmap

- перенести невыполненные чекбоксы на места реализации ([fbe672c](https://github.com/PaoloFernetti/fluxengine/commit/fbe672cc0f73708ccdf68d6d821bffd0323bf9c6))
- встроить долги этапов 1–4 в единый план работ ([306efbe](https://github.com/PaoloFernetti/fluxengine/commit/306efbe531a90c39ac301b6898d2a90b0ac30e5a))

### Internal

#### ci-cd

- аннотировать падение PSO-смоука на lavapipe ([26cba3d](https://github.com/PaoloFernetti/fluxengine/commit/26cba3d972047c19b379c6a9cb06edd3a0fcf6ed))
- job lavapipe с обязательным Vulkan-устройством ([18d1249](https://github.com/PaoloFernetti/fluxengine/commit/18d12490955e194b88c431ff6c9cf86164906baa))

## [0.4.0] - 2026-11-14

### Features

#### graphics

- Null-эмуляция содержимого и срез clear→copy→readback (WP D4) ([b6c5ed8](https://github.com/PaoloFernetti/fluxengine/commit/b6c5ed88fa4c3012b23d7b19bba7eb19e4430302))

## [0.3.0] - 2026-11-14

### Features

#### graphics

- сэмплеры в RHI по ADR-0041 (WP D3) ([49b2dfe](https://github.com/PaoloFernetti/fluxengine/commit/49b2dfe657e1a7271979eb5fcff9a000f75abc38))

### Documentation

#### adr

- ADR-0041 — сэмплеры и чтение изображений в RHI ([00fb228](https://github.com/PaoloFernetti/fluxengine/commit/00fb2281970899776fc16f5ab5c7e5cfebb1e7f3))

#### roadmap

- план закрытия долгов этапов 1–4 и план дальнейшей разработки ([9bd1bc6](https://github.com/PaoloFernetti/fluxengine/commit/9bd1bc6904f89dcad7e97e4d3afaf58cf0d90c0d))

## [0.2.1] - 2026-11-14

### Fixes

#### ci

- не отменять release-прогон и синхронизировать master отдельным job ([d018dad](https://github.com/PaoloFernetti/fluxengine/commit/d018dad00ad076d1301e5eb1993edb5b92fc4b15))

## [0.2.0] - 2026-11-14

### Features

#### ci

- add dev pipeline with tests, build, versioning and auto-merge ([4aaf278](https://github.com/PaoloFernetti/fluxengine/commit/4aaf27842921e5435084ab7366b6699c1b8344ab))

### Fixes

#### ci

- игнорировать RUSTSEC-2026-0192 для dev-зависимости winit ([f6e65ed](https://github.com/PaoloFernetti/fluxengine/commit/f6e65edaf4ed14d5ecb5cf37f2dd74169b70aa48))

#### docs

- сделать сборку docs.html независимой от EOL рабочего дерева ([8bcb697](https://github.com/PaoloFernetti/fluxengine/commit/8bcb697a9156a7c094d39321ab4ec06d68be60c4))

### Internal

#### repo

- import FluxEngine workspace with flux-release ([c1bdfc7](https://github.com/PaoloFernetti/fluxengine/commit/c1bdfc7d595e954f60ce388c9c555cc3a5a71908))
