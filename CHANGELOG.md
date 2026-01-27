# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0] 2026-01-27

### Added

- Some support for mods that change the default lock on behavior.

### Changed

- Shader crosshair visuals, now with added anti-aliasing.
- Default `crosshair.kind` to `"dot"`.
- Default `crosshair.scale_*` to `1.2`.
- Improved the `correct_screen_coords_fisheye` algorithm.
- Refactored the codebase for future maintainability.

## [0.1.12] 2026-01-26

### Added

- Better target detection to `soft_lock_on` mode.

### Fixed

- Instantly losing the Mimic Veil effect when turning the camera in first person.
- Coded Sword blade not showing up.
- Lock on reticle and tag displays with FOV correction shaders active.
- `"fisheye"` FOV correction not using the `fov_correction_strength` config value.

## [0.1.11] 2026-01-23

### Added

- `"angled"` `crosshair.kind` in erfps2.toml.
- `fov.fov_correction_cylindricity` erfps2.toml key.

### Changed

- Default `fov.fov_correction` to `"barrel"`.
- Default `fov.horizontal_fov` to 90.

### Fixed

- Barrel distortion FOV correction shader.
- Player's facing direction in soft lock on mode.
- Visibility of dual wield weapons (e.g. Ornamental Straight Swords) in first person.
- Sudden hand posture adjustments when two-handing dual wield weapons.

## [0.1.10] 2026-01-21

### Added

- "Soft" lock on option where camera movement is not restricted to the locked on target.
- Forward-only sprint option, which prevents sprinting in other directions in first person.
- `gameplay.soft_lock_on` erfps2.toml key.
- `gameplay.restricted_sprint` erfps2.toml key.

### Changed

- Sheathed weapons are now hidden (including on horseback) to prevent collisions with the camera.

## [0.1.9] 2026-01-18

### Added

- Optional (off by default) setting to improve visibility during rolls by turning the player's body transparent.
- `gameplay.unobtrusive_dodges` erfps2.toml key.

### Changed

- Camera offset and behavior by [weltensturm](https://github.com/weltensturm) in #1.
- Aiming a bow now uses the base game aim FOV and can be zoomed in.

### Fixed

- Correctly propagate current FOV to the distortion correction shader.

## [0.1.8] 2026-01-17

### Added

- Optional (off by default) head tracking for rolls.
- `gameplay.track_dodges` erfps2.toml key.

### Changed

- Ladders to no longer have head tracking.

### Fixed

- Trying to move the camera in throws snapping the look direction after the animation finishes.
- Ladders completely locking the camera movement.
- `libhotpatch` reloads correctly write to the log file.

## [0.1.7] 2026-01-16

### Added

- Support for executable version 1.16.1.1 (Japanese game distribution).
- `gameplay.start_in_first_person` erfps2.toml key.
- `gameplay.prioritize_lock_on` erfps2.toml key.

### Changed

- Errors now bring up a native pop-up window.

### Fixed

- Crosshair not disappearing when zooming with a bow.

## [0.1.6] 2026-01-15

### Added

- Ability to influence move direction when attacking.
- `gameplay.unlocked_movement` erfps2.toml key.

### Changed

- Camera head offsets to reduce clipping.

### Fixed

- Being able to unintentionally rotate when a gesture animation loops.

## [0.1.5] 2026-01-13

### Added

- Freelook when holding down **interact**.

### Changed

- The camera is offset on a pivot at the base of the neck.
- Looking up and down is less restrictive.

### Fixed

- VFX (like weapon buffs) not showing up close to the camera.

## [0.1.4] 2026-01-13

### Added

- Ability to scale the crosshair in erfps2.toml.
- `crosshair.scale_x` and `crosshair.scale_y` erfps2.toml keys.
- `crosshair.kind` erfps2.toml key.

### Changed

- Locking on is always prioritized over switching perspectives.

### Removed

- `crosshair.crosshair_kind` erfps2.toml key (renamed to `crosshair.kind`).

### Fixed

- Correctly disable crosshair in cutscenes.
- Compatibility with RemoveVignette.dll.
- Custom shader failing to enable in certain circumstances.

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
