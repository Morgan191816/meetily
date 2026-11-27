import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const mockedPaths = [
  '@tauri-apps/api/core',
  '@tauri-apps/api/event',
  'sonner',
  '../../src/contexts/ConfigContext',
  '../../src/components/ModelSettingsModal',
  '../../src/components/SummaryLanguageSettings',
  '../../src/components/ui/select',
  '../../src/components/ui/switch',
];
const originals = await Promise.all(mockedPaths.map(async path =>
  [path, { ...await import(path) }] as const));
afterAll(() => { for (const [path, exports] of originals) mock.module(path, () => exports); });

const templates = [
  { id: 'general', name: '通用纪要', description: '' },
  { id: 'standup', name: '例会/周会', description: '' },
  { id: 'review', name: '项目评审', description: '' },
  { id: 'client', name: '客户沟通', description: '' },
  { id: 'retro', name: '复盘', description: '' },
];

const invoke = mock(async (command: string, _args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_list_minutes_templates') return templates;
  if (command === 'api_get_default_minutes_template') return 'general';
  if (command === 'api_set_default_minutes_template') return undefined;
  return undefined;
});
mock.module('@tauri-apps/api/core', () => ({ invoke }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));

const successes: string[] = [];
mock.module('sonner', () => ({
  toast: {
    success: (message: string) => { successes.push(message); },
    error: () => {},
    info: () => {},
  },
}));
mock.module('../../src/contexts/ConfigContext', () => ({
  useConfig: () => ({ isAutoSummary: false, toggleIsAutoSummary: () => {} }),
}));
mock.module('../../src/components/ModelSettingsModal', () => ({
  ModelSettingsModal: () => <div>模型配置</div>,
}));
mock.module('../../src/components/SummaryLanguageSettings', () => ({
  SummaryLanguageSettings: () => null,
}));
mock.module('../../src/components/ui/switch', () => ({ Switch: () => null }));
// 用原生 select 承载 Radix Select，便于在 react-test-renderer 里触发 onValueChange
mock.module('../../src/components/ui/select', () => ({
  Select: ({ value, onValueChange, children }: any) => (
    <select value={value} onChange={(event) => onValueChange?.(event.target.value)}>{children}</select>
  ),
  SelectContent: ({ children }: any) => <>{children}</>,
  SelectItem: ({ value, children }: any) => <option value={value}>{children}</option>,
  SelectTrigger: () => null,
  SelectValue: () => null,
}));

const { SummaryModelSettings } = await import('../../src/components/SummaryModelSettings');

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  successes.length = 0;
  invoke.mockClear();
});

async function renderSettings() {
  await act(async () => {
    renderer = create(<SummaryModelSettings />);
  });
  await act(async () => {});
}

test('默认纪要模板下拉渲染 5 个选项并选中当前默认', async () => {
  await renderSettings();

  const select = renderer!.root.findByType('select');
  expect(select.findAllByType('option')).toHaveLength(5);
  expect(select.props.value).toBe('general');
});

test('选择模板后调用保存命令并提示', async () => {
  await renderSettings();

  const select = renderer!.root.findByType('select');
  await act(async () => {
    select.props.onChange({ target: { value: 'review' } });
  });

  expect(invoke.mock.calls).toContainEqual([
    'api_set_default_minutes_template',
    { templateId: 'review' },
  ]);
  expect(successes).toContain('默认模板已保存');
});
