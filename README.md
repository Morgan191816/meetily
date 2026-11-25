# Morgan Meet

Morgan Meet 是一款本地优先的会议助手：在本机完成麦克风与系统声音的双轨采集、语音转写，并生成结构化会议纪要。会议音频与转写文本默认留在本机，适合中文会议与对数据隐私有要求的团队。

## 功能

- **本地双轨采集**：同时录制麦克风与系统声音，支持设备选择、混音与回放。
- **本地转写**：默认 SenseVoice（sherpa-onnx）在本机离线转写中文，保留 Whisper 备选，无需云端 ASR。
- **会议纪要**：经统一模型配置（OpenAI / Anthropic 兼容）生成 05 纪要 12 区块、06 任务导入 Excel 12 列与 07 质检报告，含溯源标注。
- **三视图**：逐字稿、纪要、质检分栏查看；旧版纪要只读保留，可一键重新生成。
- **隐私优先**：音频与转写默认不离开本机；仅在生成纪要时才把纠错后的逐字稿发送到你配置的模型服务。

## 安装

从 [Releases](https://github.com/Morgan191816/meetily/releases) 下载安装包（MSI 或 NSIS）。SenseVoice 模型体积较大，作为单独附件分发：把 `sensevoice-model.zip` 解压到 `%APPDATA%\com.morgan.meet\models\sense-voice\`（目录内应有 `model.int8.onnx` 与 `tokens.txt`），或设置环境变量 `MORGAN_MEET_MODELS_DIR` 指向模型目录。

首次启动后在「设置 → 纪要模型配置」填写自定义服务地址、密钥与模型名。

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

- `frontend/`：Next.js/React 界面与 Rust/Tauri 原生核心（音频、持久化、转写、纪要编排）。
- `frontend/src-tauri/`：Tauri 后端，Rust 命令与事件。
- `docs/`：架构、构建与平台说明。贡献指南见 [CONTRIBUTING.md](CONTRIBUTING.md)，AI 代理约定见 [AGENTS.md](AGENTS.md)。

## License

Morgan Meet 以 MIT 许可证发布，许可证原文见 [LICENSE.md](LICENSE.md)（该文件即上游 MIT 原文）。

本项目基于 [Meetily](https://github.com/Zackriya-Solutions/meetily)（MIT，Copyright (c) 2024 Zackriya Solutions）修改而来，保留其上游署名。

随包分发与链接的第三方组件及其许可证见 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)。
