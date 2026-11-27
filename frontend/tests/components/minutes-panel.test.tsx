import { afterAll, afterEach, describe, expect, mock, test } from 'bun:test';
import type { ReactNode } from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// Bun 在测试文件间共享模块 mock；本套件结束后还原。
const originalCore = { ...await import('@tauri-apps/api/core') };
const originalBlockNote = { ...await import('../../src/components/AISummary/BlockNoteSummaryView') };
const originalDialog = { ...await import('../../src/components/ui/dialog') };
const originalSelect = { ...await import('../../src/components/ui/select') };
const originalDropdown = { ...await import('../../src/components/ui/dropdown-menu') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('@/components/AISummary/BlockNoteSummaryView', () => originalBlockNote);
  mock.module('../../src/components/ui/dialog', () => originalDialog);
  mock.module('../../src/components/ui/select', () => originalSelect);
  mock.module('../../src/components/ui/dropdown-menu', () => originalDropdown);
});

const templates = [
  { id: 'general', name: '通用纪要', description: '' },
  { id: 'standup', name: '例会/周会', description: '' },
  { id: 'review', name: '项目评审', description: '' },
  { id: 'client', name: '客户沟通', description: '' },
  { id: 'retro', name: '复盘', description: '' },
];
let templatesFail = false;

const invoke = mock(async (command: string, _args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_export_minutes_excel') return 'C:/app-data/exports/meeting-a.xlsx';
  if (command === 'api_export_minutes_markdown') return 'C:/app-data/exports/meeting-a.md';
  if (command === 'api_export_minutes_html') return 'C:/app-data/exports/meeting-a.html';
  if (command === 'api_get_minutes_html') return '<!doctype html><html><body>预览内容</body></html>';
  if (command === 'open_database_folder') return undefined;
  if (command === 'api_list_minutes_templates') {
    if (templatesFail) throw new Error('模板加载失败');
    return templates;
  }
  if (command === 'api_get_default_minutes_template') {
    if (templatesFail) throw new Error('模板加载失败');
    return 'general';
  }
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ invoke }));

// 用原生 select 承载 Radix Select，便于在 react-test-renderer 里触发 onValueChange
mock.module('../../src/components/ui/select', () => ({
  Select: ({ value, onValueChange, children, disabled }: any) => (
    <select value={value} onChange={(event) => onValueChange?.(event.target.value)} disabled={disabled}>
      {children}
    </select>
  ),
  SelectContent: ({ children }: any) => <>{children}</>,
  SelectItem: ({ value, children }: any) => <option value={value}>{children}</option>,
  SelectTrigger: () => null,
  SelectValue: () => null,
}));

// Radix DropdownMenu 依赖 portal；测试里直接把菜单项渲染成按钮，便于点击。
mock.module('../../src/components/ui/dropdown-menu', () => ({
  DropdownMenu: ({ children }: any) => <div>{children}</div>,
  DropdownMenuTrigger: ({ children }: any) => <>{children}</>,
  DropdownMenuContent: ({ children }: any) => <div>{children}</div>,
  DropdownMenuSeparator: () => null,
  DropdownMenuItem: ({ children, onSelect, disabled }: any) => (
    <button type="button" disabled={disabled} onClick={() => onSelect?.()}>
      {children}
    </button>
  ),
}));

// Radix Dialog 依赖浏览器 DOM；测试只关心按钮与 iframe。
const Container = ({ children }: { children?: ReactNode }) => <div>{children}</div>;
mock.module('../../src/components/ui/dialog', () => ({
  Dialog: Container,
  DialogContent: Container,
  DialogHeader: Container,
  DialogTitle: Container,
}));

// BlockNote 编辑器依赖浏览器 DOM；测试只关心面板是否把 markdown 交给渲染器。
mock.module('@/components/AISummary/BlockNoteSummaryView', () => ({
  BlockNoteSummaryView: ({
    summaryData,
    editable,
  }: {
    summaryData: { markdown?: string } | null;
    editable?: boolean;
  }) => <pre data-editable={String(editable ?? true)}>{summaryData?.markdown ?? ''}</pre>,
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
  templatesFail = false;
  invoke.mockClear();
});

/** 等工具栏的模板列表与默认模板加载完成。 */
async function flushTemplateLoad() {
  await act(async () => {});
}

describe('MinutesPanel', () => {
  test('无纪要时显示空态与生成按钮', async () => {
    const onGenerate = mock(() => {});
    await act(async () => {
      renderer = create(minutesPanel({ onGenerate }));
    });

    expect(textOf(renderer!.toJSON())).toContain('尚未生成会议纪要');
    // 空态只保留一个「生成纪要」按钮（工具栏在无纪要时不重复展示）
    expect(buttons(renderer!).filter((button) => textOf(button.children).includes('生成纪要'))).toHaveLength(1);

    await act(async () => {
      buttonByText(renderer!, '生成纪要').props.onClick();
    });
    expect(onGenerate).toHaveBeenCalledTimes(1);
  });

  test('工具栏渲染 5 个模板选项并默认选中全局默认', async () => {
    await act(async () => {
      renderer = create(minutesPanel());
    });
    await flushTemplateLoad();

    const select = renderer!.root.findByType('select');
    expect(select.findAllByType('option')).toHaveLength(5);
    expect(select.props.value).toBe('general');
  });

  test('已有纪要时默认选中其模板并提示覆盖', async () => {
    await act(async () => {
      renderer = create(minutesPanel({ minutes: '# 会议纪要', templateId: 'client' }));
    });
    await flushTemplateLoad();

    const select = renderer!.root.findByType('select');
    expect(select.props.value).toBe('client');
    expect(textOf(renderer!.toJSON())).toContain('重新生成会覆盖当前纪要');
  });

  test('选择模板后生成把模板 id 传给 onGenerate', async () => {
    const onGenerate = mock(() => {});
    await act(async () => {
      renderer = create(minutesPanel({ onGenerate }));
    });
    await flushTemplateLoad();

    const select = renderer!.root.findByType('select');
    await act(async () => {
      select.props.onChange({ target: { value: 'review' } });
    });
    await act(async () => {
      buttonByText(renderer!, '生成纪要').props.onClick();
    });

    expect(onGenerate).toHaveBeenCalledWith('review');
  });

  test('有纪要时工具栏生成也带上所选模板', async () => {
    const onGenerate = mock(() => {});
    await act(async () => {
      renderer = create(minutesPanel({ minutes: '# 会议纪要', onGenerate }));
    });
    await flushTemplateLoad();

    const select = renderer!.root.findByType('select');
    await act(async () => {
      select.props.onChange({ target: { value: 'standup' } });
    });
    await act(async () => {
      buttonByText(renderer!, '生成纪要').props.onClick();
    });

    expect(onGenerate).toHaveBeenCalledWith('standup');
  });

  test('模板列表加载失败时隐藏下拉，生成仍可用', async () => {
    templatesFail = true;
    const onGenerate = mock(() => {});
    await act(async () => {
      renderer = create(minutesPanel({ onGenerate }));
    });
    await flushTemplateLoad();

    expect(renderer!.root.findAllByType('select')).toHaveLength(0);

    await act(async () => {
      buttonByText(renderer!, '生成纪要').props.onClick();
    });
    expect(onGenerate).toHaveBeenCalledWith(undefined);
  });

  test('有纪要时渲染 markdown 区块', async () => {
    await act(async () => {
      renderer = create(minutesPanel({ minutes: '## 决策事项\n\n- 决策-01：确认预算' }));
    });

    expect(textOf(renderer!.toJSON())).toContain('决策事项');
  });

  test('无新纪要时只读渲染旧版摘要并提示', async () => {
    await act(async () => {
      renderer = create(minutesPanel({
        legacySummary: {
          Decisions: { title: '决策事项', blocks: [{ id: 'b1', type: 'text', content: '确认预算', color: '' }] },
        },
      }));
    });

    const text = textOf(renderer!.toJSON());
    expect(text).toContain('此纪要由旧版生成，重新生成将使用新版纪要');
    expect(text).toContain('决策事项');
    expect(text).toContain('确认预算');
    expect(renderer!.root.findByType('pre').props['data-editable']).toBe('false');
  });

  test('旧纪要会议可以重新生成，导出仍禁用', async () => {
    const onGenerate = mock(() => {});
    await act(async () => {
      renderer = create(minutesPanel({
        legacySummary: {
          Decisions: { title: '决策事项', blocks: [{ id: 'b1', type: 'text', content: '确认预算', color: '' }] },
        },
        onGenerate,
      }));
    });

    const generateButtons = buttons(renderer!).filter((button) => textOf(button.children).includes('生成纪要'));
    expect(generateButtons).toHaveLength(1);
    expect(buttonByText(renderer!, '导出').props.disabled).toBe(true);

    await act(async () => {
      generateButtons[0].props.onClick();
    });
    expect(onGenerate).toHaveBeenCalledTimes(1);
  });

  test('旧版 summary_json 纪要只读渲染并提示', async () => {
    await act(async () => {
      renderer = create(minutesPanel({
        legacySummary: {
          summary_json: [{ id: 'b1', type: 'paragraph', content: [{ type: 'text', text: '旧纪要内容' }] }],
        },
      }));
    });

    const text = textOf(renderer!.toJSON());
    expect(text).toContain('此纪要由旧版生成，重新生成将使用新版纪要');
    expect(renderer!.root.findByType('pre').props['data-editable']).toBe('false');
  });

  test('纪要面板以只读模式渲染编辑器', async () => {
    await act(async () => {
      renderer = create(minutesPanel({ minutes: '# 会议纪要' }));
    });

    expect(renderer!.root.findByType('pre').props['data-editable']).toBe('false');
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

  test('有纪要时可导出 Markdown，调用后端', async () => {
    await act(async () => {
      renderer = create(minutesPanel({ minutes: '# 会议纪要' }));
    });

    expect(buttonByText(renderer!, '导出 Markdown').props.disabled).toBe(false);

    await act(async () => {
      buttonByText(renderer!, '导出 Markdown').props.onClick();
    });

    expect(invoke.mock.calls).toContainEqual([
      'api_export_minutes_markdown',
      { meetingId: 'meeting-a' },
    ]);
  });

  test('有纪要时可导出 HTML，调用后端', async () => {
    await act(async () => {
      renderer = create(minutesPanel({ minutes: '# 会议纪要' }));
    });

    expect(buttonByText(renderer!, '导出 HTML').props.disabled).toBe(false);

    await act(async () => {
      buttonByText(renderer!, '导出 HTML').props.onClick();
    });

    expect(invoke.mock.calls).toContainEqual([
      'api_export_minutes_html',
      { meetingId: 'meeting-a' },
    ]);
  });

  test('无纪要时导出入口禁用', async () => {
    await act(async () => {
      renderer = create(minutesPanel());
    });

    expect(buttonByText(renderer!, '导出').props.disabled).toBe(true);
  });

  test('有纪要时可预览，调用后端并渲染 iframe', async () => {
    await act(async () => {
      renderer = create(minutesPanel({ minutes: '# 会议纪要' }));
    });

    expect(buttonByText(renderer!, '预览').props.disabled).toBe(false);

    await act(async () => {
      buttonByText(renderer!, '预览').props.onClick();
    });
    // 等 invoke 的 promise 落地
    await act(async () => {});

    expect(invoke.mock.calls).toContainEqual([
      'api_get_minutes_html',
      { meetingId: 'meeting-a' },
    ]);
    const iframe = renderer!.root.findByType('iframe');
    expect(iframe.props.srcDoc).toContain('预览内容');
    expect(iframe.props.sandbox).toBe('');
  });
});

describe('QualityPanel', () => {
  test('质检 Tab 渲染质检报告', async () => {
    await act(async () => {
      renderer = create(<QualityPanel quality={'# 质检报告\n\n## 溯源统计\n\n- 全部通过'} />);
    });

    expect(textOf(renderer!.toJSON())).toContain('质检报告');
    expect(textOf(renderer!.toJSON())).toContain('溯源统计');
    expect(renderer!.root.findByType('pre').props['data-editable']).toBe('false');
  });

  test('无质检报告时显示空态', async () => {
    await act(async () => {
      renderer = create(<QualityPanel quality={null} />);
    });

    expect(textOf(renderer!.toJSON())).toContain('尚未生成质检报告');
  });
});
