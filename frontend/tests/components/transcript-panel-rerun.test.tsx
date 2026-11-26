import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { DiarizationStatus } from '../../src/hooks/useDiarization';

// 只关心「重新分离」按钮的可见性，子组件与上下文都用最小替身
const originalCore = { ...await import('@tauri-apps/api/core') };
const originalEvents = { ...await import('@tauri-apps/api/event') };
const originalDiarization = { ...await import('../../src/hooks/useDiarization') };
const originalConfig = { ...await import('../../src/contexts/ConfigContext') };
const originalView = { ...await import('../../src/components/VirtualizedTranscriptView') };

const invoke = mock(async (command: string): Promise<unknown> => {
  if (command === 'api_get_meeting_speakers') return [];
  if (command === 'diarization_get_status') return { state: 'available' };
  return null;
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
mock.module('@tauri-apps/api/event', () => ({ ...originalEvents, listen: async () => () => {} }));
mock.module('@/contexts/ConfigContext', () => ({ ...originalConfig, useConfig: () => ({ betaFeatures: {} }) }));
mock.module('@/components/VirtualizedTranscriptView', () => ({ ...originalView, VirtualizedTranscriptView: () => null }));

let status: DiarizationStatus = 'idle';
const rerun = mock(async () => {});
mock.module('../../src/hooks/useDiarization', () => ({
  ...originalDiarization,
  useDiarization: () => ({
    status,
    progress: 0,
    unavailable: status === 'unavailable',
    error: null,
    empty: false,
    nameKey: 0,
    labelKey: 0,
    bumpNames: () => {},
    bumpLabels: () => {},
    rerun,
    cancel: async () => {},
    checkAvailability: async () => {},
  }),
}));

afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('@tauri-apps/api/event', () => originalEvents);
  mock.module('../../src/hooks/useDiarization', () => originalDiarization);
  mock.module('@/contexts/ConfigContext', () => originalConfig);
  mock.module('@/components/VirtualizedTranscriptView', () => originalView);
});

const { TranscriptPanel } = await import('../../src/components/MeetingDetails/TranscriptPanel');

let renderer: ReactTestRenderer | undefined;
beforeEach(() => {
  status = 'idle';
});
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
});

const transcripts = [{
  id: 't1',
  text: 'hello',
  timestamp: '00:00',
  audio_start_time: 0,
  audio_end_time: 1,
  confidence: 0.9,
  speaker: 'S1',
}];

async function show(nextStatus: DiarizationStatus) {
  status = nextStatus;
  await act(async () => {
    renderer = create(
      <TranscriptPanel
        transcripts={transcripts as never}
        customPrompt=""
        onPromptChange={() => {}}
        onCopyTranscript={() => {}}
        onOpenMeetingFolder={async () => {}}
        isRecording={false}
        meetingId="meeting-a"
      />,
    );
  });
  await act(async () => {});
}

function rerunButton() {
  return renderer!.root.findAll(
    (node) => typeof node.type === 'string' && node.children.join('') === '重新分离',
  );
}

describe('重新分离入口', () => {
  test('done 状态仍可重跑', async () => {
    await show('done');
    expect(rerunButton().length).toBeGreaterThan(0);
  });

  test('processing 状态不显示重跑', async () => {
    await show('processing');
    expect(rerunButton()).toHaveLength(0);
  });
});
