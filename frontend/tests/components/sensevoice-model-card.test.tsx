import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const listeners = new Map<string, (event: { payload: unknown }) => void>();
let status: { state: string; progress: number | null; path: string } = {
  state: 'missing',
  progress: null,
  path: '',
};

const originals = await Promise.all(['@tauri-apps/api/core', '@tauri-apps/api/event'].map(async path =>
  [path, { ...await import(path) }] as const));
afterAll(() => { for (const [path, exports] of originals) mock.module(path, () => exports); });

mock.module('@tauri-apps/api/core', () => ({
  invoke: mock(async (command: string) => {
    if (command === 'sensevoice_get_status') return status;
    return undefined;
  }),
}));
mock.module('@tauri-apps/api/event', () => ({
  listen: mock(async (event: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(event, handler);
    return () => listeners.delete(event);
  }),
}));

const { SenseVoiceModelCard } = await import('../../src/components/SenseVoiceModelCard');

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  listeners.clear();
});

test('模型未下载时显示下载按钮', async () => {
  status = { state: 'missing', progress: null, path: '' };
  await act(async () => { renderer = create(<SenseVoiceModelCard />); });
  expect(JSON.stringify(renderer!.toJSON())).toContain('下载模型（约 228 MB）');
});

test('进度到 100% 不当作完成，完成只认 complete 事件', async () => {
  status = { state: 'missing', progress: null, path: '' };
  await act(async () => { renderer = create(<SenseVoiceModelCard />); });

  // 模型与 tokens 分两段下载：第一段到 100% 后进度会回到 0%
  await act(async () => { listeners.get('sensevoice-download-progress')!({ payload: { progress: 100 } }); });
  const downloading = JSON.stringify(renderer!.toJSON());
  expect(downloading).toContain('正在下载');
  expect(downloading).not.toContain('已就绪');

  status = { state: 'available', progress: null, path: '' };
  await act(async () => { listeners.get('sensevoice-download-complete')!({ payload: {} }); });
  expect(JSON.stringify(renderer!.toJSON())).toContain('已就绪');
});
