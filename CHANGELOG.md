# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial Cargo workspace with the `vnce` crate (imported as `vnce`).
- Project documentation, license and tooling configuration.
- `vnce-core` (`vnce_core`): geometry (`Position`, `Size`, `Rect`), `Color` with hex/name
  parsing and downsampling to 256 / 16 colors, `Style` and `Modifiers`, `Cell`, and a
  Unicode-aware `Buffer` with wide-character handling and diffing.
