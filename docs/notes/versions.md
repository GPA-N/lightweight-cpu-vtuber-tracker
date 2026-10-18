# Version notes (task 1.7, cpu-face-tracking-spike)

Research date: 2026-10-17. Sources: crates.io API, ort docs (ort.pyke.io), raw.githubusercontent.com (ort and nokhwa files), github.com (nokhwa issues), windows-docs-rs, static.rust-lang.org (stable toolchain manifest), and learn.microsoft.com (via search). Nothing was built or installed.

## 1. Bevy and bevy_vrm1

- `bevy_vrm1` latest 0.10.x is **0.10.0** (created 2026-10-03). The 0.10 line has only this version (checked against the 25-entry versions list: https://crates.io/api/v1/crates/bevy_vrm1/versions). License MIT OR Apache-2.0. Source: https://crates.io/api/v1/crates/bevy_vrm1
- Bevy version required by `bevy_vrm1` 0.10.0: **bevy ^0.19** (normal dependency, `default_features = false`). Source: https://crates.io/api/v1/crates/bevy_vrm1/0.10.0/dependencies
- Latest Bevy stable is **0.20.0** (created 2026-10-08, rust_version 1.97.1). Bevy 0.19 line max is **0.19.1** (rust_version 1.95.0, created 2026-08-13). So `bevy_vrm1` 0.10.0 is one minor behind Bevy. The project must pin `bevy = "0.19"` to match it. Sources: https://crates.io/api/v1/crates/bevy , https://crates.io/api/v1/crates/bevy/0.19.1
- Bevy license: MIT OR Apache-2.0.

## 2. ort and ONNX Runtime

- ort latest version: **2.0.0-rc.14** (created 2026-10-12, rust_version 1.92; confirmed in https://crates.io/api/v1/crates/ort/versions). `max_stable_version` is null, so no non-prerelease 2.0.x exists on crates.io. License MIT OR Apache-2.0. Source: https://crates.io/api/v1/crates/ort
- ort docs state: "These docs are for the latest version of ort, `2.0.0-rc.14`", install line `ort = "=2.0.0-rc.14"`. Source: https://ort.pyke.io/
- ONNX Runtime mapping from ort docs: "ort v2.0.0+ → ONNX Runtime v1.31.0". The page also says "2.0 ships with ONNX Runtime 1.31." **Label caveat:** under semver, `2.0.0-rc.14` is lower than `2.0.0`, so the "v2.0.0+" row does not strictly cover the rc. The docs do not name an ONNX Runtime version for rc.14 directly. Treat "rc.14 → ONNX Runtime 1.31" as the docs' 2.0 line, not as a stated pairing. Source: https://ort.pyke.io/misc/version-mapping
- ort default features, copied from the `[features]` section of `ort` crate's Cargo.toml at tag `v2.0.0-rc.14`: `default = [ "std", "ndarray", "tracing", "download-binaries", "tls-native", "copy-dylibs", "api-31" ]`. Source: https://raw.githubusercontent.com/pykeio/ort/v2.0.0-rc.14/Cargo.toml
- Backend features are opt-in and not in `default`: `cuda`, `tensorrt`, `directml`, plus others (`openvino`, `coreml`, `rocm`, ...). Source: same Cargo.toml.
- Windows MSVC and CPU-only:
  - Default: ort "download[s] statically-linked ONNX Runtime binaries built by pyke.io" (feature `download-binaries`, enabled by default). Source: https://ort.pyke.io/misc/prebuilt-binaries
  - Prebuilt page: "All Windows builds come with the DirectML EP enabled." The page does not say whether a CPU-only x86_64-pc-windows-msvc prebuilt exists, and does not say whether DirectML.dll must ship beside the exe. Source: https://ort.pyke.io/misc/prebuilt-binaries
  - Static linking: build ONNX Runtime with `build.sh` without `--build_shared_lib`, then set `ORT_LIB_PATH`. Source: https://ort.pyke.io/setup/linking
  - Dynamic options: `load-dynamic` (uses `ORT_DYLIB_PATH`; `load-dynamic = [ "std", "preload-dylibs", "ort-sys/disable-linking" ]` in the same Cargo.toml), or compile-time dynamic with `ORT_PREFER_DYNAMIC_LINK=1`. Linking page: "For Windows, this is either somewhere on the `PATH`, or in the same folder as the executable." Source: https://ort.pyke.io/setup/linking
  - Execution provider feature names: `cuda` (`ep::CUDA`), `tensorrt` (`ep::TensorRT`), `directml` (`ep::DirectML`). None is in the default feature set, so they stay off unless enabled. Source: https://ort.pyke.io/perf/execution-providers and the Cargo.toml above.
  - CPU fallback: "ort will silently fail and fall back to executing on the CPU if all execution providers fail to register." Source: https://ort.pyke.io/perf/execution-providers
  - CPU-only build = default features, with no `cuda`, `tensorrt`, or `directml` feature enabled. No disable flag found in the docs fetched (not proven absent); those features are simply not enabled.
- Windows MSVC build support: NOT FOUND. The platforms page had no Windows section (`https://ort.pyke.io/setup/platforms`).
- Nightly toolchain: NOT FOUND. No nightly requirement appeared in the pages fetched; rust_version 1.92 is a stable release.

## 3. Other crates (latest version, license, MSRV)

| Crate | Latest | License | rust_version (MSRV) | Nightly? | Source |
|---|---|---|---|---|---|
| nalgebra | 0.35.0 (2026-05-24) | Apache-2.0 | 1.89.0 | Not stated | https://crates.io/api/v1/crates/nalgebra |
| nokhwa | 0.10.11 (2026-05-15) | Apache-2.0 (crates.io) | null (not stated) | Not stated | https://crates.io/api/v1/crates/nokhwa/0.10.11 |
| nokhwa-bindings-windows | 0.4.6 (2026-05-15) | Apache-2.0 | null | Not stated | https://crates.io/api/v1/crates/nokhwa-bindings-windows |
| windows | 0.62.2 (2025-10-06) | MIT OR Apache-2.0 | 1.82 | Not stated | https://crates.io/api/v1/crates/windows |
| serde_json | 1.0.154 (2026-10-11) | MIT OR Apache-2.0 | 1.71 | Not stated | https://crates.io/api/v1/crates/serde_json |
| image | 0.25.10 (2026-03-10) | MIT OR Apache-2.0 | 1.88.0 | Not stated | https://crates.io/api/v1/crates/image |

## 4. Camera crate comparison (Windows Media Foundation)

- `nokhwa` 0.10.11 features, from crates.io version JSON (https://crates.io/api/v1/crates/nokhwa/0.10.11):
  - `default = ["decoding"]`. `input-native` is NOT default, so the camera backend must be enabled explicitly.
  - `input-native = ["input-avfoundation", "input-v4l", "input-msmf"]`.
  - `input-msmf = ["nokhwa-bindings-windows"]` (the Windows MF backend).
- `nokhwa-bindings-windows` 0.4.6 depends on `windows ^0.62` with features including `Win32_Media_MediaFoundation`. Source: https://crates.io/api/v1/crates/nokhwa-bindings-windows/0.4.6/dependencies
- Raw `windows` 0.62.2 (last published 2025-10-06): gives MF bindings directly, but the camera enumeration and capture code must be written by hand. crates.io API `max_version` and `max_stable_version` are both `"0.62.2"` (https://crates.io/api/v1/crates/windows).
- Recommendation: use `nokhwa` 0.10.11 with features `default-features = false, features = ["input-msmf"]` (or `input-native`). It already targets the same `windows ^0.62` line. Fallback if it fails the spike: raw `windows` 0.62.2 MF bindings.
- Known open nokhwa issues (GitHub, state checked 2026-10-17; all open):
  - #110 "Returns incorrect frame rate on Windows" (https://github.com/l1npengtul/nokhwa/issues/110)
  - #144 "Wrong `frame_rate` on Windows" (https://github.com/l1npengtul/nokhwa/issues/144)
  - #165 "threaded camera deadlock on stop stream" (https://github.com/l1npengtul/nokhwa/issues/165)
  - #184 "Corrupted image" (https://github.com/l1npengtul/nokhwa/issues/184). Body starts with a code snippet; no black-frame statement in the fetched text. Symptom not confirmed.
- Caveat: nokhwa has no stated MSRV (crates.io `rust_version` null).

## 5. windows crate features (process memory, DXGI)

- Process memory: `GetProcessMemoryInfo` and `PROCESS_MEMORY_COUNTERS_EX` appear under `windows::Win32::System::ProcessStatus` in the windows-docs-rs docs. Source: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/ProcessStatus/index.html . The feature name `Win32_System_ProcessStatus` is inferred from the module path. The docs page did not show the feature-requirement text, so this is NOT confirmed from a source.
- DXGI: `IDXGIAdapter3` and `QueryVideoMemoryInfo` exist in the windows bindings. Source: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Graphics/Dxgi/index.html . The feature name `Win32_Graphics_Dxgi` is inferred from the module path. The feature-requirement text was not shown, so it is NOT confirmed from a source.
- Media Foundation: `IMFActivate` is listed under `Win32::Media::MediaFoundation`. Source: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Media/MediaFoundation/index.html . The feature name `Win32_Media_MediaFoundation` is confirmed by the nokhwa-bindings-windows dependency list.
- Win32 API docs: `GetProcessMemoryInfo` needs PROCESS_QUERY_INFORMATION or PROCESS_QUERY_LIMITED_INFORMATION. Source: https://learn.microsoft.com/en-us/windows/win32/api/psapi (via search, not fetched directly).
- `IDXGIAdapter3::QueryVideoMemoryInfo` reports Budget and CurrentUsage in bytes. Source: https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_4/nf-dxgi1_4-idxgiadapter3-queryvideomemoryinfo (via search).

## 6. Toolchain pin

- Stable channel at fetch time: **rustc 1.99.1** (`version = "1.99.1 (58cf8f9c3 2026-10-13)"`, `date = "2026-10-15"`). Source: https://static.rust-lang.org/dist/channel-rust-stable.toml
- Highest MSRV among crates that declare one: **1.95.0** (bevy 0.19.1). Others: ort 1.92, nalgebra 1.89.0, image 1.88.0, windows 1.82, serde_json 1.71. nokhwa and nokhwa-bindings-windows declare null, so their MSRV is unknown. Bevy 0.20 would need 1.97.1, but it is not the chosen line.
- Cargo.toml MSRV (`rust-version` field): **`rust-version = "1.95.0"`**. Reason: the minimum compiler that builds bevy 0.19.1, the highest declared MSRV in the chosen set. Not a toolchain pin.
- Toolchain pin (`rust-toolchain.toml`): **`channel = "1.99.1"`**. Reason: current stable at fetch time (2026-10-17), which is above the 1.95.0 MSRV. Pinning a specific version keeps builds reproducible. Source for the stable version: https://static.rust-lang.org/dist/channel-rust-stable.toml

## NOT FOUND

- Whether DirectML.dll must ship beside the exe for ort's prebuilt Windows binaries: not stated on https://ort.pyke.io/misc/prebuilt-binaries or https://ort.pyke.io/setup/linking.
- What the `copy-dylibs` feature copies, and where: the feature name is in ort's default list, but its effect was not stated on pages fetched.
- Feature-requirement text (exact feature name) for `windows` modules `System::ProcessStatus`, `Graphics::Dxgi`: docs.rs and windows-docs-rs pages did not show it. docs.rs `latest` and `0.62.2` paths returned 404.
- ort Windows MSVC CPU-only prebuilt availability: NOT FOUND (prebuilt-binaries page does not list per-platform CPU builds).
- ort explicit "disable CUDA/TensorRT/DirectML" flag: NOT FOUND. Only opt-in features are documented.
- ort non-prerelease 2.0.0: NOT FOUND (no stable release on crates.io).
- nokhwa MSRV number: NOT FOUND (crates.io `rust_version` null).
- nokhwa last commit date: NOT FOUND. Its GitHub repo raw Cargo.toml (`main` and `senpai` branch paths) returned 404; features were taken from crates.io instead.
- Nightly toolchain requirement for any crate above: NOT FOUND.
- `nokhwa-bindings-windows` rust_version: null on crates.io.
