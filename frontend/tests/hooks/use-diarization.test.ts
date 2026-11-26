import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import { createElement } from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// Bun 在测试文件间共享模块 mock；本套件结束后还原，避免影响其它测试。
const originalCore = { ...await import('@tauri-apps/api/core') };
const originalEvents = { ...await import('@tauri-apps/api/event') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('@tauri-apps/api/event', () => originalEvents);
});

type EventHandler = (event: { payload: unknown }) => void;
const listeners = new Map<string, EventHandler>();
const listen = mock(async (name: string, handler: EventHandler) => {
  listeners.set(name, handler);
  return () => listeners.delete(name);
});
mock.module('@tauri-apps/api/event', () => ({ listen }));

const invoke = mock(async (command: string): Promise<unknown> => {
  if (command === 'diarization_get_status') return { state: 'available' };
  if (command === 'api_cancel_diarization') return true;
  if (command === 'api_run_diarization') return 2;
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ invoke }));

const { availabilityAfterCheck, failureAfterRun, useDiarization } = await import('../../src/hooks/useDiarization');

let state: ReturnType<typeof useDiarization>;
function View({ meetingId }: { meetingId: string }) {
  state = useDiarization(meetingId);
  return createElement('output', null, state.status);
}

let renderer: ReactTestRenderer | null = null;

async function show(meetingId = 'meeting-a') {
  await act(async () => {
    renderer = create(createElement(View, { meetingId }));
  });
  // 等 hook 的 listen 注册完成
  await act(async () => {});
}

async function emit(name: string, payload: unknown) {
  const handler = listeners.get(name);
  if (!handler) throw new Error(`未注册事件监听：${name}`);
  await act(async () => {
    handler({ payload });
  });
}

beforeEach(() => {
  renderer = null;
  listeners.clear();
  invoke.mockClear();
  listen.mockClear();
});

afterEach(async () => {
  if (renderer) {
    await act(async () => {
      renderer!.unmount();
    });
  }
});

describe('availabilityAfterCheck', () => {
  test('模型缺失且空闲时提示未安装', () => {
    expect(availabilityAfterCheck('missing', 'idle')).toBe('unavailable');
  });

  test('正在分离或已完成时不覆盖状态', () => {
    expect(availabilityAfterCheck('missing', 'processing')).toBe('processing');
    expect(availabilityAfterCheck('missing', 'done')).toBe('done');
  });

  test('模型可用时保持原状态', () => {
    expect(availabilityAfterCheck('available', 'idle')).toBe('idle');
    expect(availabilityAfterCheck('downloading', 'idle')).toBe('idle');
  });
});

describe('failureAfterRun', () => {
  test('失败把状态从 processing 复位为空闲并保留错误文案', () => {
    expect(failureAfterRun('解码音频失败：坏文件')).toEqual({
      status: 'idle',
      progress: 0,
      error: '解码音频失败：坏文件',
    });
  });
});

describe('useDiarization', () => {
  test('失败事件复位进度并暴露错误，进度条不会永久停住', async () => {
    await show();
    await emit('diarization-progress', { meetingId: 'meeting-a', stage: 'decoding', percent: 5 });
    expect(state.status).toBe('processing');
    expect(state.progress).toBe(5);

    await emit('diarization-error', { meetingId: 'meeting-a', error: '分离任务失败' });
    expect(state.status).toBe('idle');
    expect(state.progress).toBe(0);
    expect(state.error).toBe('分离任务失败');
  });

  test('其他会议的失败事件被忽略', async () => {
    await show();
    await emit('diarization-error', { meetingId: 'meeting-b', error: '别的会议' });
    expect(state.error).toBeNull();
  });

  test('零标签结果标记空结果，有标签时清除', async () => {
    await show();
    await emit('speakers-updated', { meetingId: 'meeting-a', labels: 0 });
    expect(state.empty).toBe(true);
    expect(state.status).toBe('done');

    await emit('speakers-updated', { meetingId: 'meeting-a', labels: 3 });
    expect(state.empty).toBe(false);
    expect(state.labelKey).toBe(2);
  });

  test('新一轮进度清除上一轮错误与空结果', async () => {
    await show();
    await emit('speakers-updated', { meetingId: 'meeting-a', labels: 0 });
    await emit('diarization-progress', { meetingId: 'meeting-a', stage: 'decoding', percent: 5 });
    expect(state.empty).toBe(false);
    expect(state.error).toBeNull();
  });
});
