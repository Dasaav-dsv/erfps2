# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.3] 2026-01-12

### Added

- Changelog and Discord server links to README.md

### Fixed

- More chestpiece hoods (e.g. Black Knife) not being hidden in first person.
- FOV correction applying to the crosshair.

## [0.1.2] 2026-01-12

### Added

- New crosshair kinds: "none", "cross", "dot", "circle".
- `crosshair.crosshair_kind` erfps2.toml key.

### Removed

- `crosshair.enabled` erfps2.toml key.

### Fixed

- Chestpiece hoods (e.g. Black Knife or Gravekeeper) not being hidden in first person.
- Hand posture being adjusted for players other than the main player.

## [0.1.1] 2026-01-11

### Added

- Config `erfps2.toml` with live reloading.

### Changed

- The game now starts in first person by default.

### Fixed

- Camera drift in first person.
- Crosshair staying enabled in third person.
