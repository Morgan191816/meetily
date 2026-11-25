import { describe, expect, test } from 'bun:test';
import { hasVisibleSummaryContent, parseSummaryContent, summaryToMarkdown } from '../../src/lib/summary-content';

describe('summary content validation', () => {
  test('accepts visible markdown and rejects whitespace or reasoning markers', () => {
    expect(hasVisibleSummaryContent({ markdown: '# Summary\nVisible' })).toBe(true);
    expect(hasVisibleSummaryContent({ markdown: '   ' })).toBe(false);
    expect(hasVisibleSummaryContent({ markdown: '<think>private</think>' })).toBe(false);
    expect(hasVisibleSummaryContent({
      markdown: '<thinking class="internal">private</thinking>',
    })).toBe(false);
    expect(hasVisibleSummaryContent({ markdown: '<thinker>Visible</thinker>' })).toBe(true);
  });

  test('validates BlockNote and legacy summary content', () => {
    expect(hasVisibleSummaryContent({ summary_json: [] })).toBe(false);
    expect(hasVisibleSummaryContent({
      summary_json: [{ content: [{ type: 'text', text: 'Visible' }] }],
    })).toBe(true);
    expect(hasVisibleSummaryContent({
      Decisions: { blocks: [{ content: '   ' }] },
    })).toBe(false);
    expect(hasVisibleSummaryContent({
      Decisions: { blocks: [{ content: 'Ship it' }] },
    })).toBe(true);
  });

  test('parses one historical double-encoded payload', () => {
    expect(parseSummaryContent('{"markdown":"Visible"}')).toEqual({ markdown: 'Visible' });
    expect(parseSummaryContent('{not json')).toBeNull();
  });
});

describe('summaryToMarkdown', () => {
  test('converts legacy sections to markdown bullets', () => {
    expect(summaryToMarkdown({
      Decisions: {
        title: '决策事项',
        blocks: [{ content: '确认预算' }, { content: '   ' }],
      },
    })).toBe('## 决策事项\n\n- 确认预算');
  });

  test('returns null when only summary_json blocks exist', () => {
    expect(summaryToMarkdown({
      summary_json: [{ content: [{ type: 'text', text: 'Visible' }] }],
    })).toBeNull();
  });

  test('does not output MeetingName metadata', () => {
    const markdown = summaryToMarkdown({
      MeetingName: 'InternalName',
      Decisions: { title: '决策事项', blocks: [{ content: '确认预算' }] },
    });
    expect(markdown).toContain('决策事项');
    expect(markdown).not.toContain('InternalName');
  });
});
