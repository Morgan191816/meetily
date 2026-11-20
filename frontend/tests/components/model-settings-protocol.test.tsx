import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestInstance, type ReactTestRenderer } from 'react-test-renderer';

// bun test 环境没有 localStorage；组件在保存时会写 providerModelMap 缓存
if (typeof (globalThis as any).localStorage === 'undefined') {
  const store = new Map<string, string>();
  (globalThis as any).localStorage = {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
  };
}

const mockedPaths = [
  '@tauri-apps/api/core',
  'sonner',
  '../../src/contexts/ConfigContext',
  '../../src/components/Sidebar/SidebarProvider',
  '../../src/contexts/OllamaDownloadContext',
  '../../src/components/ui/select',
];
const originals = await Promise.all(mockedPaths.map(async path =>
  [path, { ...await import(path) }] as const));
afterAll(() => { for (const [path, exports] of originals) mock.module(path, () => exports); });

let listModelsFails = false;
let listModelsGate: Promise<void> | null = null;
const invoke = mock(async (command: string, args?: unknown) => {
  if (command === 'api_list_custom_models') {
    if (listModelsFails) throw '无法连接 http://gw:4000：connection refused';
    if (listModelsGate) await listModelsGate;
    return ['claude-a', 'claude-b'];
  }
  return undefined;
});

mock.module('@tauri-apps/api/core', () => ({ invoke }));
mock.module('sonner', () => ({ toast: { error: () => {}, success: () => {}, info: () => {} } }));
mock.module('../../src/contexts/ConfigContext', () => ({ useConfig: () => null }));
mock.module('../../src/components/Sidebar/SidebarProvider', () => ({
  useSidebar: () => ({ serverAddress: null }),
}));
mock.module('../../src/contexts/OllamaDownloadContext', () => ({
  useOllamaDownload: () => ({
    isDownloading: () => false,
    getProgress: () => undefined,
    downloadingModels: new Set<string>(),
  }),
}));
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

const { ModelSettingsModal } = await import('../../src/components/ModelSettingsModal');

const baseConfig = {
  provider: 'custom-openai' as const,
  model: 'gpt-4o',
  whisperModel: 'large-v3',
  customOpenAIEndpoint: 'http://gw:4000',
  customOpenAIModel: 'gpt-4o',
  customOpenAIApiKey: 'sk-test',
};

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  listModelsFails = false;
  listModelsGate = null;
});

async function renderModal() {
  await act(async () => {
    renderer = create(
      <ModelSettingsModal
        modelConfig={baseConfig}
        setModelConfig={() => {}}
        onSave={() => {}}
        skipInitialFetch
      />
    );
  });
}

function inputById(id: string) {
  return renderer!.root.findAllByType('input').find(node => node.props.id === id);
}

async function selectProtocol(protocol: 'openai' | 'anthropic') {
  const select = renderer!.root.findAllByType('select').find(node => node.props.value === 'openai');
  await act(async () => { select!.props.onChange({ target: { value: protocol } }); });
}

function instanceText(node: ReactTestInstance): string {
  return node.children.map(child => typeof child === 'string' ? child : instanceText(child)).join('');
}

function findButton(text: string) {
  return renderer!.root.findAllByType('button').find(node => instanceText(node).includes(text));
}

async function clickFetchModels() {
  await act(async () => { await findButton('获取模型')!.props.onClick(); });
}

test('切换协议会清空已选模型并显示协议提示', async () => {
  await renderModal();
  expect(JSON.stringify(renderer!.toJSON())).toContain('/v1/chat/completions');

  await selectProtocol('anthropic');

  const json = JSON.stringify(renderer!.toJSON());
  expect(json).toContain('/v1/messages');
  expect(json).not.toContain('/v1/chat/completions');
  expect(inputById('custom-model')!.props.value).toBe('');
  // URL 与 Key 保留
  expect(inputById('custom-endpoint')!.props.value).toBe('http://gw:4000');
  expect(inputById('custom-api-key')!.props.value).toBe('sk-test');
});

test('拉取失败时保留手动输入', async () => {
  listModelsFails = true;
  await renderModal();

  await clickFetchModels();

  const json = JSON.stringify(renderer!.toJSON());
  expect(json).toContain('无法连接');
  const errorNode = renderer!.root.findAllByType('p').find(node =>
    JSON.stringify(node.props.children).includes('无法连接'));
  expect(JSON.stringify(errorNode!.props.children)).not.toContain('sk-test');

  const modelInput = inputById('custom-model');
  expect(modelInput).toBeDefined();
  await act(async () => { modelInput!.props.onChange({ target: { value: 'my-model' } }); });
  expect(inputById('custom-model')!.props.value).toBe('my-model');
});

test('拉取成功用当前协议并把模型填入下拉', async () => {
  await renderModal();
  await selectProtocol('anthropic');
  await clickFetchModels();

  const calls = invoke.mock.calls.filter(([command]) => command === 'api_list_custom_models');
  expect(calls.at(-1)![1]).toEqual({
    endpoint: 'http://gw:4000',
    apiKey: 'sk-test',
    protocol: 'anthropic',
  });
  const json = JSON.stringify(renderer!.toJSON());
  expect(json).toContain('claude-a');
  expect(json).toContain('claude-b');
});

test('拉取在途时切协议，旧结果不回填', async () => {
  let release!: () => void;
  listModelsGate = new Promise<void>(resolve => { release = resolve; });
  await renderModal();

  let fetchDone!: Promise<void>;
  await act(async () => {
    fetchDone = findButton('获取模型')!.props.onClick();
    await Promise.resolve();
  });
  await selectProtocol('anthropic');
  await act(async () => { release(); await fetchDone; });

  expect(inputById('custom-model')!.props.value).toBe('');
  expect(JSON.stringify(renderer!.toJSON())).not.toContain('claude-a');
});

test('保存时把当前协议传给保存命令', async () => {
  await renderModal();
  await selectProtocol('anthropic');
  await act(async () => {
    inputById('custom-model')!.props.onChange({ target: { value: 'claude-a' } });
  });
  await act(async () => { await findButton('保存')!.props.onClick(); });

  const calls = invoke.mock.calls.filter(([command]) => command === 'api_save_custom_openai_config');
  expect(calls.at(-1)![1]).toMatchObject({
    endpoint: 'http://gw:4000',
    model: 'claude-a',
    protocol: 'anthropic',
  });
});
