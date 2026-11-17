# Third-Party Notices

Morgan Meet is a modified version of [Meetily](https://github.com/Zackriya-Solutions/meetily),
licensed under the MIT License, Copyright (c) 2024 Zackriya Solutions. The upstream
license text is reproduced in [LICENSE.md](LICENSE.md).

This product includes the following third-party components. License identifiers follow
each project's own declaration; the authoritative license text ships with the component
or its upstream distribution.

| Component | License | Notes |
| --- | --- | --- |
| Tauri / wry / tao | MIT / Apache-2.0 | Application shell and webview bindings |
| whisper-rs / whisper.cpp | MIT | Whisper speech recognition |
| Silero VAD (`silero-rs`) | MIT | Voice activity detection |
| CPAL | Apache-2.0 | Cross-platform audio I/O |
| FFmpeg (`binaries/ffmpeg`) | LGPL-2.1+ or GPL-2.0-or-later, depending on the build | Redistributed as a bundled binary; the applicable license text and source code access must be provided with any distributed build |
| BlockNote (`@blocknote/*`) | MPL-2.0 | Used unmodified |
| ONNX Runtime (`ort`) | MIT | Only needed for Parakeet models. After the Parakeet engine is removed, ONNX Runtime is no longer bundled or distributed |

Other Rust and JavaScript dependencies, with their resolved versions, are listed in
`Cargo.lock` and `frontend/pnpm-lock.yaml`. Each dependency remains under its own
upstream license.

## FFmpeg redistribution

The bundled FFmpeg binary is built with a specific feature set, and its license
(LGPL-2.1-or-later, or GPL-2.0-or-later if GPL-only components are enabled) depends on that build.
Anyone redistributing Morgan Meet with this binary must include the corresponding
license text and make the FFmpeg source code for that exact build available, as required
by the license under which it was built.
