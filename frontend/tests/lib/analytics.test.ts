import { afterAll, expect, mock, test } from 'bun:test';
const originalCore = { ...await import('@tauri-apps/api/core') };
afterAll(() => { mock.module('@tauri-apps/api/core', () => originalCore); });

const invokeMock = mock(async (_command: string, _args?: unknown) => null);

mock.module('@tauri-apps/api/core', () => ({ ...originalCore,
  invoke: invokeMock,
}));
// The analytics module must load after the Tauri invoke mock is registered.
const { Analytics } = await import('../../src/lib/analytics');

test('analytics never invokes a Tauri command', async () => {
  await Analytics.init();
  await Analytics.identify('user');
  await Analytics.startSession('user');
  await Analytics.track('event', { a: 'b' });
  await Analytics.trackSummaryGenerationCompleted('ollama', 'model', true, 1.999, 'error');
  await Analytics.trackTranscriptionError('GAIA HTTP 401: unauthorized');
  await Analytics.trackTranscriptionSuccess(3);
  await Analytics.trackMeetingCompleted('meeting-1', {
    duration_seconds: 60,
    transcript_segments: 2,
    transcript_word_count: 20,
    words_per_minute: 20,
    meetings_today: 1,
  });
  await Analytics.endSession();
  await Analytics.cleanup();

  expect(invokeMock.mock.calls).toEqual([]);
  expect(await Analytics.isEnabled()).toBe(false);
  expect(await Analytics.startSession('user')).toBeNull();
});
