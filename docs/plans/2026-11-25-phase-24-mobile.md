# Plan: Phase 24 — Mobile platforms (Android and iOS)

**Date:** 2026-11-25
**Status:** Planned, waiting for the user's approval to start (the user asked on
2026-11-25 for mobile as a platform)

## Goal

Games made with vnce should run on phones and tablets. Phones have no terminal, so a
mobile game runs in the native window of Phase 18. The libraries under the window
already support both platforms (`winit` for Android and iOS, `wgpu` with Vulkan / GLES
on Android and Metal on iOS, `cpal` with AAudio / Oboe and Core Audio), but vnce has
never been built for them and has no touch input.

## What it adds

1. **Feasibility first:** build `vnce-window` and an example for Android and iOS,
   measure what works (start-up, drawing, sound, save files) and list what is missing;
   the rest of the plan is adjusted to what this finds.
2. **The app life cycle:** suspend and resume (the GPU surface is lost and made again),
   rotation and size changes, the back button on Android, and `SaveFile` in the app's
   data folder.
3. **Touch input:** touches as engine events (start, move, end, in cells and in
   pixels), simple gestures (tap, drag, swipe), and on-screen controls (a d-pad,
   buttons, a stick) drawn over the game and bound to `ActionMap` actions.
4. **Layout on phones:** safe areas (notches, rounded corners), a font size chosen from
   the screen's density, portrait and landscape.
5. **Packaging:** building an APK / AAB (with `cargo-apk` or `xbuild`) and an iOS app
   (Xcode project); a section in the shipping guide.
6. **One showcase on a phone:** one of the examples playable with touch.

## Steps (one commit each)

1. Plan details and the feasibility build (Android first; iOS needs a Mac).
2. Life cycle and save files.
3. Touch events and gestures.
4. On-screen controls and `ActionMap` bindings.
5. Safe areas, density and orientation.
6. Packaging and the shipping guide.
7. The showcase on a phone, changelog, closing.

## Dependencies (to be approved)

- Possibly `android-activity` (MIT / Apache-2.0, through `winit`) and `ndk` crates for
  Android; nothing extra is expected for iOS.

## Risks / open questions

- Building for Android needs the Android SDK and NDK and the Rust targets
  (`aarch64-linux-android` …); building for iOS needs a Mac with Xcode and an Apple
  developer account to run on a device. Installing them is the user's decision.
- The CI can build for Android on Linux; iOS needs a macOS runner.
- Terminal-only features (graphics protocols, pixel modes in cells) do not apply on
  phones; games must use `vnce::run` with the `window` feature.
