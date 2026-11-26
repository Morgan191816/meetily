import { expect, test } from 'bun:test';
import { availabilityAfterCheck } from '../../src/hooks/useDiarization';

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
