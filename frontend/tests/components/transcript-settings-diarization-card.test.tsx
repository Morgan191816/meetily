import React from 'react';
import { afterAll, expect, mock, test } from 'bun:test';
import { act, create } from 'react-test-renderer';

// Bun 在测试文件间共享模块 mock；本套件结束后还原。
const originals = await Promise.all([
  '@tauri-apps/api/core',
  '@tauri-apps/api/event',
  '../../src/components/WhisperModelManager',
  '../../src/components/SenseVoiceModelCard',
  '../../src/components/ui/select',
].map(async path => [path, { ...await import(path) }] as const));
afterAll(() => { for (const [path, exports] of originals) mock.module(path, () => exports); });

mock.module('@tauri-apps/api/core', () => ({
  invoke: mock(async (command: string) => {
    if (command === 'diarization_get_status') return { state: 'missing', progress: null, path: '' };
    return undefined;
  }),
}));
mock.module('@tauri-apps/api/event', () => ({ listen: mock(async () => () => {}) }));

const Placeholder = ({ label }: { label: string }) => <div>{label}</div>;
mock.module('../../src/components/WhisperModelManager', () => ({
  ModelManager: () => <Placeholder label="whisper-model-manager" />,
}));
mock.module('../../src/components/SenseVoiceModelCard', () => ({
  SenseVoiceModelCard: () => <Placeholder label="sensevoice-model-card" />,
}));
// Radix Select 的 Trigger 依赖 window；测试只关心卡片是否挂载。
const Container = ({ children }: { children?: React.ReactNode }) => <div>{children}</div>;
mock.module('../../src/components/ui/select', () => ({
  Select: Container,
  SelectContent: Container,
  SelectItem: Container,
  SelectTrigger: Container,
  SelectValue: Container,
}));

const { TranscriptSettings } = await import('../../src/components/TranscriptSettings');

// 回归守护：说话人分离模型卡与转写 provider 无关，不能只挂在 senseVoice 分支里。
test('provider 为 localWhisper 时也渲染说话人分离模型卡', async () => {
  let renderer;
  await act(async () => {
    renderer = create(
      <TranscriptSettings
        transcriptModelConfig={{ provider: 'localWhisper', model: 'base', apiKey: null }}
        setTranscriptModelConfig={() => {}}
      />,
    );
  });
  await act(async () => {});

  const json = JSON.stringify(renderer!.toJSON());
  expect(json).toContain('whisper-model-manager');
  expect(json).not.toContain('sensevoice-model-card');
  expect(json).toContain('说话人分离模型');

  await act(async () => renderer!.unmount());
});
