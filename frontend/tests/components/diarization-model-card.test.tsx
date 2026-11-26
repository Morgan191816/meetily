import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const listeners = new Map<string, (event: { payload: unknown }) => void>();
let status: { state: string; progress: number | null; path: string } = {
  state: 'missing',
  progress: null,
  path: '',
};
let statusFails = false;
const invoked: string[] = [];

const originals = await Promise.all(['@tauri-apps/api/core', '@tauri-apps/api/event'].map(async path =>
  [path, { ...await import(path) }] as const));
afterAll(() => { for (const [path, exports] of originals) mock.module(path, () => exports); });

mock.module('@tauri-apps/api/core', () => ({
  invoke: mock(async (command: string) => {
    invoked.push(command);
    if (command === 'diarization_get_status') {
      if (statusFails) throw new Error('ipc failed');
      return status;
    }
    return undefined;
  }),
}));
mock.module('@tauri-apps/api/event', () => ({
  listen: mock(async (event: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(event, handler);
    return () => listeners.delete(event);
  }),
}));

const { DiarizationModelCard } = await import('../../src/components/DiarizationModelCard');

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  listeners.clear();
  invoked.length = 0;
  statusFails = false;
});

test('模型未下载时显示下载按钮', async () => {
  status = { state: 'missing', progress: null, path: '' };
  await act(async () => { renderer = create(<DiarizationModelCard />); });
  expect(JSON.stringify(renderer!.toJSON())).toContain('下载模型（约 34 MB）');
});

test('模型已就绪时显示已就绪', async () => {
  status = { state: 'available', progress: null, path: '' };
  await act(async () => { renderer = create(<DiarizationModelCard />); });
  expect(JSON.stringify(renderer!.toJSON())).toContain('已就绪');
});

test('读取状态失败时显示重试，而不是伪装成未下载', async () => {
  statusFails = true;
  await act(async () => { renderer = create(<DiarizationModelCard />); });
  let json = JSON.stringify(renderer!.toJSON());
  expect(json).toContain('读取模型状态失败，请重试');
  expect(json).not.toContain('下载模型（');

  statusFails = false;
  status = { state: 'available', progress: null, path: '' };
  const retry = renderer!.root.findAllByType('button').find(button =>
    JSON.stringify(button.props.children).includes('重试'));
  await act(async () => { retry!.props.onClick(); });
  json = JSON.stringify(renderer!.toJSON());
  expect(json).toContain('已就绪');
});

test('点击下载调用命令，进度事件更新百分比，complete 事件后回到已就绪', async () => {
  status = { state: 'missing', progress: null, path: '' };
  await act(async () => { renderer = create(<DiarizationModelCard />); });

  const download = renderer!.root.findAllByType('button').find(button =>
    JSON.stringify(button.props.children).includes('下载模型'));
  await act(async () => { download!.props.onClick(); });
  expect(invoked).toContain('diarization_download_models');

  await act(async () => { listeners.get('diarization-download-progress')!({ payload: { progress: 70 } }); });
  expect(JSON.stringify(renderer!.toJSON())).toContain('正在下载… 70%');

  status = { state: 'available', progress: null, path: '' };
  await act(async () => { listeners.get('diarization-download-complete')!({ payload: {} }); });
  expect(JSON.stringify(renderer!.toJSON())).toContain('已就绪');
});
