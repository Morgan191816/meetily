import { afterEach, expect, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { ConfidenceIndicator } from '../../src/components/ConfidenceIndicator';

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
});

test('估算置信度会明确标注估算', async () => {
  await act(async () => {
    renderer = create(<ConfidenceIndicator confidence={0.8} estimated />);
  });
  const tree = renderer!.toJSON() as any;
  expect(tree.props.title).toContain('估算');
});

test('没有置信度时不渲染指示器', async () => {
  await act(async () => {
    renderer = create(<ConfidenceIndicator confidence={undefined} />);
  });
  expect(renderer!.toJSON()).toBeNull();
});

test('真实置信度不标注估算', async () => {
  await act(async () => {
    renderer = create(<ConfidenceIndicator confidence={0.8} />);
  });
  const tree = renderer!.toJSON() as any;
  expect(tree.props.title).toContain('置信度 80%');
  expect(tree.props.title).not.toContain('估算');
});
