# Morgan Meet

Morgan Meet 是一款本地优先的会议助手：在本机完成麦克风与系统声音的双轨采集、语音转写，并生成结构化会议纪要、行动项与 Excel 导出。会议音频、转写文本与模型推理默认全部留在本机，适合中文会议与对数据隐私有要求的团队。

## 功能

- **本地双轨采集**：同时录制麦克风与系统声音，支持设备选择、混音与回放。
- **本地转写**：基于 Whisper（whisper.cpp / whisper-rs）在本机离线转写，无需云端 ASR。
- **会议纪要**：自动生成摘要、决策、行动项、风险与待决问题，支持 Markdown 纪要、Excel 行动项导出。
- **会议问答**：基于本地模型的会议内容检索与问答。
- **隐私优先**：音频与转写默认不离开本机；仅在显式配置云端摘要服务时才发送转写文本。

## 从源码构建

前置条件：pnpm 9.15.9、Node 20、Rust 1.99.1，以及各平台的原生构建依赖。详细步骤见 [构建说明](docs/BUILDING.md)。

```bash
cd frontend
pnpm install --frozen-lockfile
pnpm tauri:build
```

开发运行使用 `pnpm tauri:dev`。GPU 加速（CUDA / Vulkan / Metal / CoreML）需选择对应的构建特性，见 [GPU 加速说明](docs/GPU_ACCELERATION.md)。

## 测试

```bash
# Rust 后端测试
cd frontend/src-tauri && cargo test

# 前端测试
cd frontend && bun test
```

## 项目结构

- `frontend/`：Next.js/React 界面与 Rust/Tauri 原生核心（音频、持久化、转写、摘要编排）。
- `frontend/src-tauri/`：Tauri 后端，Rust 命令与事件。
- `docs/`：架构、构建与平台说明。贡献指南见 [CONTRIBUTING.md](CONTRIBUTING.md)，AI 代理约定见 [AGENTS.md](AGENTS.md)。

## License

Morgan Meet 以 MIT 许可证发布，许可证原文见 [LICENSE.md](LICENSE.md)（该文件即上游 MIT 原文）。

本项目基于 [Meetily](https://github.com/Zackriya-Solutions/meetily)（MIT，Copyright (c) 2024 Zackriya Solutions）修改而来，保留其上游署名。

随包分发与链接的第三方组件及其许可证见 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)。
