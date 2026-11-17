import { expect, test } from 'bun:test';
import { cleanStopWords } from '../../src/components/VirtualizedTranscriptView';
import { charsPerTick } from '../../src/hooks/useTranscriptStreaming';

test('removes standalone Chinese filler words', () => {
  expect(cleanStopWords('嗯，我们开始吧')).toBe('我们开始吧');
  expect(cleanStopWords('啊，这个方案不错')).toBe('这个方案不错');
  expect(cleanStopWords('嗯，好的')).toBe('好的');
});

test('keeps Chinese words that carry meaning', () => {
  expect(cleanStopWords('那个就是重点')).toBe('那个就是重点');
  expect(cleanStopWords('这笔金额是额外支出')).toBe('这笔金额是额外支出');
  expect(cleanStopWords('额度还剩多少')).toBe('额度还剩多少');
  expect(cleanStopWords('我天啊这个方案真不错')).toBe('我天啊这个方案真不错');
});

test('reveals CJK text twice as fast as Latin text', () => {
  expect(charsPerTick('我们开始吧')).toBe(2);
  expect(charsPerTick('let us start')).toBe(1);
});
