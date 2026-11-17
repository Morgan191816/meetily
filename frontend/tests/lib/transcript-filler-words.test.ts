import { expect, test } from 'bun:test';
import { cleanStopWords } from '../../src/components/VirtualizedTranscriptView';
import { charsPerTick } from '../../src/hooks/useTranscriptStreaming';

test('removes single-syllable Chinese filler words', () => {
  expect(cleanStopWords('嗯，我们开始吧')).toBe('我们开始吧');
  expect(cleanStopWords('啊这个方案哦不错')).toBe('这个方案不错');
});

test('keeps Chinese words that carry meaning', () => {
  expect(cleanStopWords('那个就是重点')).toBe('那个就是重点');
});

test('reveals CJK text twice as fast as Latin text', () => {
  expect(charsPerTick('我们开始吧')).toBe(2);
  expect(charsPerTick('let us start')).toBe(1);
});
