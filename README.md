![Discord](https://img.shields.io/discord/1011219400772309062?label=Discord)

# [FPS] ELDEN RING First Person Souls (Ver.2)

A ground-up rewrite of the [original first person mod](https://www.nexusmods.com/eldenring/mods/3266) for ELDEN RING.

This mod aims to convert the game into a proper first person experience, with the rewrite focusing on reliability and maintainability.

## Features

- Intuitive first person controls, strafing and movement based on the camera direction, now with full lock on support.
- Custom barrel distortion shader to reduce FOV distortion in first person inspired by [an article by Giliam de Carpentier](https://www.decarpentier.nl/lens-distortion).
- Accurate free aim (complete with a crosshair).
- Full body animations with minimal camera clipping, while preserving the shadow of the player's helmet and head.
- Runtime config editing (erfps2.toml) that is instantly reflected in game.

## Controls

Hold down **interact** (like you would when bringing up the item pouch or two-handing) and press **lock on** on keyboard and mouse or controller.

## Installation

Install [me3](https://me3.help/en/latest/) and download the latest stable version from the [Releases](https://github.com/Dasaav-dsv/erfps2/releases) tab or the newest build from the [build artifacts](https://github.com/Dasaav-dsv/erfps2/actions).

Launch the me3 mod profile directly or with the me3 CLI:

```
me3 launch -g eldenring -p erfps2.me3
```

You may edit `erfps2.toml` to your preference. Keep it in the same directory as `erfps2.dll` and do not remove any fields.

## Changelog

[CHANGELOG.md](CHANGELOG.md)

## Developer quickstart

1. Download and install [DXC](https://github.com/microsoft/DirectXShaderCompiler/releases/latest) (DirectX Shader Compiler) and place it in your `PATH`, or set the `DXC_PATH` environment variable to the `dxc` executable path when building erfps2.

2. Download and install [me3](https://me3.help/en/latest/).

3. Clone the erfps2 repository and initialize the submodules:

```
git clone https://github.com/Dasaav-dsv/erfps2.git
cd erfps2
git submodule update --init --recursive
```

4. To use `libhotpatch` for live code reloads build in debug mode (with `cargo build`) and copy `erfps2.dll` elsewhere from `target/x86_64-pc-windows-msvc/debug`. Subsequent `cargo build` invocations will reload the erfps2 DLL while the game is running. See the `run` bash script for an example.
