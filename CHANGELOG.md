# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

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
