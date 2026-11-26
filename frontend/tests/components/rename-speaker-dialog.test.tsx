import React from 'react';
import { afterAll, afterEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// Bun 在测试文件间共享模块 mock；本套件结束后还原。
const originals = await Promise.all(
  ['@tauri-apps/api/core', '../../src/components/ui/dialog'].map(
    async (path) => [path, { ...await import(path) }] as const,
  ),
);
afterAll(() => { for (const [path, exports] of originals) mock.module(path, () => exports); });

const invoke = mock(async (command: string, _args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_get_roster_names') return ['曹磊'];
  return undefined;
});
mock.module('@tauri-apps/api/core', () => ({ invoke }));

// Radix Dialog 依赖浏览器 DOM；测试只关心提交时调用了哪个命令。
const Container = ({ children }: { children?: React.ReactNode }) => <div>{children}</div>;
mock.module('../../src/components/ui/dialog', () => ({
  Dialog: Container,
  DialogContent: Container,
  DialogHeader: Container,
  DialogTitle: Container,
}));

const { RenameSpeakerDialog } = await import('../../src/components/Transcript/RenameSpeakerDialog');

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  invoke.mockClear();
});

describe('RenameSpeakerDialog', () => {
  test('输入名字点保存后调用 api_rename_speaker 一次', async () => {
    let saved = 0;
    let closed = 0;
    await act(async () => {
      renderer = create(
        <RenameSpeakerDialog
          meetingId="meeting-a"
          label="SPEAKER_00"
          current=""
          onClose={() => { closed += 1; }}
          onSaved={() => { saved += 1; }}
        />,
      );
    });
    // 等 OA 名单加载完成
    await act(async () => {});

    const input = renderer!.root.findByType('input');
    await act(async () => {
      input.props.onChange({ target: { value: '曹磊' } });
    });

    const save = renderer!.root.findAllByType('button').find((node) => node.props.children === '保存');
    expect(save).toBeDefined();
    await act(async () => {
      save!.props.onClick();
    });

    expect(invoke.mock.calls.filter(([command]) => command === 'api_rename_speaker')).toEqual([
      ['api_rename_speaker', { meetingId: 'meeting-a', label: 'SPEAKER_00', name: '曹磊' }],
    ]);
    expect(saved).toBe(1);
    expect(closed).toBe(1);
  });
});
