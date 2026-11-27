import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
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

let getMinutes: () => Promise<unknown>;
let generateMinutes: () => Promise<unknown>;
let cancelMinutes: () => Promise<boolean>;
const invoke = mock(async (command: string, _args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_get_minutes') return getMinutes();
  if (command === 'api_generate_minutes') return generateMinutes();
  if (command === 'api_cancel_minutes') return cancelMinutes();
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ invoke }));

const { useMinutesGeneration } = await import('../../src/hooks/useMinutesGeneration');

const record = (overrides: Record<string, unknown> = {}) => ({
  meetingId: 'meeting-a',
  markdown: '# 会议纪要',
  qualityMarkdown: '# 质检报告',
  tasks: [],
  traceabilityPass: true,
  validationPass: true,
  createdAt: '2026-08-04T10:00:00Z',
  updatedAt: '2026-08-04T10:00:00Z',
  ...overrides,
});

let state: ReturnType<typeof useMinutesGeneration>;
function View({ meetingId }: { meetingId: string }) {
  state = useMinutesGeneration(meetingId);
  return <output>{JSON.stringify({ status: state.status, progress: state.progress, minutes: state.minutes, quality: state.quality, templateId: state.templateId, error: state.error, loaded: state.loaded })}</output>;
}

let renderer: ReactTestRenderer | null = null;

async function show(meetingId = 'meeting-a') {
  await act(async () => {
    renderer = create(<View meetingId={meetingId} />);
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
  getMinutes = async () => null;
  generateMinutes = async () => record();
  cancelMinutes = async () => true;
});

afterEach(async () => {
  if (renderer) {
    await act(async () => {
      renderer!.unmount();
    });
  }
});

describe('useMinutesGeneration', () => {
  test('首次加载完成前 loaded 为 false，完成后为 true', async () => {
    let resolveGet!: (value: unknown) => void;
    getMinutes = () => new Promise((resolve) => {
      resolveGet = resolve;
    });

    await show();
    expect(state.loaded).toBe(false);

    await act(async () => {
      resolveGet(null);
    });
    expect(state.loaded).toBe(true);
  });

  test('首次加载失败时 loaded 仍为 true', async () => {
    getMinutes = async () => {
      throw new Error('数据库不可用');
    };

    await show();
    expect(state.loaded).toBe(true);
  });

  test('取消未命中且生成随后失败时状态为 error', async () => {
    let rejectGenerate!: (error: Error) => void;
    generateMinutes = () => new Promise((_resolve, reject) => {
      rejectGenerate = reject;
    });
    cancelMinutes = async () => false;
    await show();

    let generation!: Promise<void>;
    await act(async () => {
      generation = state.generate();
    });
    expect(state.status).toBe('processing');

    let cancelled!: boolean;
    await act(async () => {
      cancelled = await state.cancel();
    });
    expect(cancelled).toBe(false);
    expect(state.status).toBe('processing');

    // 未命中取消时事件流必须恢复，否则在途运行的进度被静默
    await emit('minutes-progress', { meetingId: 'meeting-a', stage: 'summarizing', percent: 40 });
    expect(state.progress).toEqual({ stage: 'summarizing', percent: 40 });

    await act(async () => {
      rejectGenerate(new Error('模型不可用'));
      await generation;
    });
    expect(state.status).toBe('error');
    expect(state.error).toBe('模型不可用');
  });

  test('进度事件驱动状态迁移', async () => {
    let resolveGenerate!: (value: unknown) => void;
    generateMinutes = () => new Promise((resolve) => {
      resolveGenerate = resolve;
    });

    await show();
    expect(state.status).toBe('idle');

    let generation!: Promise<void>;
    await act(async () => {
      generation = state.generate();
    });
    expect(state.status).toBe('processing');
    expect(invoke.mock.calls).toContainEqual([
      'api_generate_minutes',
      { meetingId: 'meeting-a', templateId: undefined },
    ]);

    // 别的会议的进度必须被过滤
    await emit('minutes-progress', { meetingId: 'meeting-b', stage: 'summarizing', percent: 40 });
    expect(state.status).toBe('processing');
    expect(state.progress).toEqual({ stage: null, percent: 0 });

    await emit('minutes-progress', { meetingId: 'meeting-a', stage: 'summarizing', percent: 40 });
    expect(state.progress).toEqual({ stage: 'summarizing', percent: 40 });

    getMinutes = async () => record({ markdown: '# 新纪要', qualityMarkdown: '# 新质检' });
    await emit('minutes-progress', { meetingId: 'meeting-a', stage: 'done', percent: 100 });
    expect(state.status).toBe('completed');
    expect(state.progress).toEqual({ stage: 'done', percent: 100 });
    expect(state.minutes).toBe('# 新纪要');
    expect(state.quality).toBe('# 新质检');

    await act(async () => {
      resolveGenerate(record({ markdown: '# 新纪要' }));
      await generation;
    });
  });

  test('取消后回到 idle 且保留旧纪要', async () => {
    getMinutes = async () => record({ markdown: '# 旧纪要', qualityMarkdown: '# 旧质检' });
    await show();
    expect(state.minutes).toBe('# 旧纪要');

    let rejectGenerate!: (error: Error) => void;
    generateMinutes = () => new Promise((_resolve, reject) => {
      rejectGenerate = reject;
    });

    let generation!: Promise<void>;
    await act(async () => {
      generation = state.generate();
    });
    await emit('minutes-progress', { meetingId: 'meeting-a', stage: 'summarizing', percent: 40 });
    expect(state.status).toBe('processing');

    let cancelled!: boolean;
    await act(async () => {
      cancelled = await state.cancel();
    });
    expect(cancelled).toBe(true);
    expect(invoke.mock.calls).toContainEqual(['api_cancel_minutes', { meetingId: 'meeting-a' }]);
    expect(state.status).toBe('idle');
    expect(state.progress).toEqual({ stage: null, percent: 0 });
    expect(state.minutes).toBe('# 旧纪要');
    expect(state.quality).toBe('# 旧质检');

    // 取消后管线 Promise 的拒绝不应把状态改成 error
    await act(async () => {
      rejectGenerate(new Error('纪要生成已取消'));
      await generation;
    });
    expect(state.status).toBe('idle');
    expect(state.error).toBeNull();
    expect(state.minutes).toBe('# 旧纪要');
  });

  test('取消后旧运行成功返回不覆盖已取消的 idle', async () => {
    let resolveGenerate!: (value: unknown) => void;
    generateMinutes = () => new Promise((resolve) => {
      resolveGenerate = resolve;
    });
    await show();

    let generation!: Promise<void>;
    await act(async () => {
      generation = state.generate();
    });

    await act(async () => {
      expect(await state.cancel()).toBe(true);
    });
    expect(state.status).toBe('idle');

    // 旧运行的 Promise 成功返回时 runId 已过期，不得覆盖 idle
    await act(async () => {
      resolveGenerate(record({ markdown: '# 旧运行' }));
      await generation;
    });
    expect(state.status).toBe('idle');
    expect(state.minutes).toBeNull();
  });

  test('取消后迟到的进度事件不把状态改回 processing', async () => {
    generateMinutes = () => new Promise(() => {});
    await show();

    await act(async () => {
      void state.generate();
    });
    expect(state.status).toBe('processing');

    await act(async () => {
      expect(await state.cancel()).toBe(true);
    });
    expect(state.status).toBe('idle');

    await emit('minutes-progress', { meetingId: 'meeting-a', stage: 'summarizing', percent: 60 });
    expect(state.status).toBe('idle');
    expect(state.progress).toEqual({ stage: null, percent: 0 });
  });

  test('generate 透传所选模板并记录返回的 templateId', async () => {
    generateMinutes = async () => record({ templateId: 'review' });
    await show();

    await act(async () => {
      await state.generate('review');
    });

    expect(invoke.mock.calls).toContainEqual([
      'api_generate_minutes',
      { meetingId: 'meeting-a', templateId: 'review' },
    ]);
    expect(state.templateId).toBe('review');
  });

  test('读取已有纪要时返回其模板', async () => {
    getMinutes = async () => record({ templateId: 'client' });
    await show();

    expect(state.templateId).toBe('client');
  });

  test('旧纪要无模板时 templateId 为 null', async () => {
    getMinutes = async () => record();
    await show();

    expect(state.templateId).toBeNull();
  });

  test('minutes-error 置为失败并保留旧纪要', async () => {
    getMinutes = async () => record({ markdown: '# 旧纪要' });
    await show();

    await emit('minutes-error', { meetingId: 'meeting-b', error: '别的会议' });
    expect(state.status).toBe('idle');

    await emit('minutes-error', { meetingId: 'meeting-a', error: '模型不可用' });
    expect(state.status).toBe('error');
    expect(state.error).toBe('模型不可用');
    expect(state.minutes).toBe('# 旧纪要');
  });
});
