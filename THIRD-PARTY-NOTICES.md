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
| ONNX Runtime (`ort`) | MIT | Required by the Silero VAD runtime; bundled with the application |
| sherpa-onnx | Apache-2.0 | SenseVoice offline speech recognition runtime (statically linked) |
| SenseVoice model weights (FunAudioLLM) | FunASR Model Open Source License Agreement v1.1 | Downloaded on first use; files come from the official sherpa-onnx conversion |

Other Rust and JavaScript dependencies, with their resolved versions, are listed in
`Cargo.lock` and `frontend/pnpm-lock.yaml`. Each dependency remains under its own
upstream license.

## sherpa-onnx

- License: Apache-2.0
- Use: runtime for SenseVoice offline speech recognition
- Project: https://github.com/k2-fsa/sherpa-onnx

## SenseVoice model weights

- License: FunASR Model Open Source License Agreement v1.1
- Model name: SenseVoice (FunAudioLLM); the distributed files come from the official sherpa-onnx conversion
- Attribution: the model name and this license notice must be retained in any redistribution

## FFmpeg redistribution

The bundled FFmpeg binary is built with a specific feature set, and its license
(LGPL-2.1-or-later, or GPL-2.0-or-later if GPL-only components are enabled) depends on that build.
Anyone redistributing Morgan Meet with this binary must include the corresponding
license text and make the FFmpeg source code for that exact build available, as required
by the license under which it was built.
