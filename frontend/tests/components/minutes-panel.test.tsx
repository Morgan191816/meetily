import { afterAll, afterEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// Bun 在测试文件间共享模块 mock；本套件结束后还原。
const originalCore = { ...await import('@tauri-apps/api/core') };
const originalBlockNote = { ...await import('../../src/components/AISummary/BlockNoteSummaryView') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('@/components/AISummary/BlockNoteSummaryView', () => originalBlockNote);
});

const invoke = mock(async (command: string, _args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_export_minutes_excel') return 'C:/app-data/exports/meeting-a.xlsx';
  if (command === 'open_database_folder') return undefined;
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ invoke }));

// BlockNote 编辑器依赖浏览器 DOM；测试只关心面板是否把 markdown 交给渲染器。
mock.module('@/components/AISummary/BlockNoteSummaryView', () => ({
  BlockNoteSummaryView: ({ summaryData }: { summaryData: { markdown?: string } | null }) => (
    <pre>{summaryData?.markdown ?? ''}</pre>
  ),
}));

const { MinutesPanel } = await import('../../src/components/MeetingDetails/MinutesPanel');
const { QualityPanel } = await import('../../src/components/MeetingDetails/QualityPanel');

function textOf(node: unknown): string {
  if (node == null) return '';
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(textOf).join('');
  return textOf((node as { children?: unknown }).children);
}

function buttons(renderer: ReactTestRenderer) {
  return renderer.root.findAll((node) => node.type === 'button');
}

function buttonByText(renderer: ReactTestRenderer, text: string) {
  const found = buttons(renderer).find((button) => textOf(button.children).includes(text));
  if (!found) throw new Error(`未找到按钮：${text}`);
  return found;
}

const idleProgress = { stage: null, percent: 0 };

function minutesPanel(overrides: Partial<Parameters<typeof MinutesPanel>[0]> = {}) {
  return (
    <MinutesPanel
      meetingId="meeting-a"
      status="idle"
      progress={idleProgress}
      minutes={null}
      error={null}
      onGenerate={() => {}}
      onCancel={() => {}}
      {...overrides}
    />
  );
}

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  invoke.mockClear();
});

describe('MinutesPanel', () => {
  test('无纪要时显示空态与生成按钮', async () => {
    const onGenerate = mock(() => {});
    await act(async () => {
      renderer = create(minutesPanel({ onGenerate }));
    });

    expect(textOf(renderer!.toJSON())).toContain('尚未生成会议纪要');

    await act(async () => {
      buttonByText(renderer!, '生成纪要').props.onClick();
    });
    expect(onGenerate).toHaveBeenCalledTimes(1);
  });

  test('有纪要时渲染 markdown 区块', async () => {
    await act(async () => {
      renderer = create(minutesPanel({ minutes: '## 决策事项\n\n- 决策-01：确认预算' }));
    });

    expect(textOf(renderer!.toJSON())).toContain('决策事项');
  });

  test('导出 Excel 调用后端并提示文件位置', async () => {
    await act(async () => {
      renderer = create(minutesPanel({ minutes: '# 会议纪要' }));
    });

    await act(async () => {
      buttonByText(renderer!, '导出 Excel').props.onClick();
    });

    expect(invoke.mock.calls).toContainEqual([
      'api_export_minutes_excel',
      { meetingId: 'meeting-a' },
    ]);
  });
});

describe('QualityPanel', () => {
  test('质检 Tab 渲染质检报告', async () => {
    await act(async () => {
      renderer = create(<QualityPanel quality={'# 质检报告\n\n## 溯源统计\n\n- 全部通过'} />);
    });

    expect(textOf(renderer!.toJSON())).toContain('质检报告');
    expect(textOf(renderer!.toJSON())).toContain('溯源统计');
  });

  test('无质检报告时显示空态', async () => {
    await act(async () => {
      renderer = create(<QualityPanel quality={null} />);
    });

    expect(textOf(renderer!.toJSON())).toContain('尚未生成质检报告');
  });
});
