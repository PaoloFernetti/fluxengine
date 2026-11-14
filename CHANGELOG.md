# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

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
