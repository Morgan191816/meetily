import type { MeetingSummary } from '@/types';
import { z } from 'zod';

const REASONING_MARKER = /<\/?think(?:ing)?(?:\s+[^>]*)?>/i;
const SummaryObject = z.record(z.unknown());

function parseSummaryObject(value: unknown) {
  const parsed = SummaryObject.safeParse(value);
  return parsed.success ? parsed.data : null;
}

function containsReasoningMarker(value: unknown): boolean {
  if (Array.isArray(value)) {
    return value.some(containsReasoningMarker);
  }
  const object = parseSummaryObject(value);
  if (!object) {
    return false;
  }

  return Object.entries(object).some(([key, child]) => {
    if (typeof child === 'string') {
      return ['markdown', 'text', 'content'].includes(key) && REASONING_MARKER.test(child);
    }
    return ['summary_json', 'children', 'content', 'blocks'].includes(key)
      && containsReasoningMarker(child);
  });
}

function hasBlockText(value: unknown): boolean {
  if (typeof value === 'string') {
    return value.trim().length > 0;
  }
  if (Array.isArray(value)) {
    return value.some(hasBlockText);
  }
  const object = parseSummaryObject(value);
  return object ? [object.text, object.content, object.children].some(hasBlockText) : false;
}

function hasLegacySectionText(value: unknown): boolean {
  const section = parseSummaryObject(value);
  return Array.isArray(section?.blocks) && section.blocks.some((block) => {
    const parsedBlock = parseSummaryObject(block);
    return typeof parsedBlock?.content === 'string' && parsedBlock.content.trim().length > 0;
  });
}

export function hasVisibleSummaryContent(value: unknown): value is MeetingSummary {
  const object = parseSummaryObject(value);
  if (!object || containsReasoningMarker(value)) {
    return false;
  }
  if ('markdown' in object) {
    return typeof object.markdown === 'string' && object.markdown.trim().length > 0;
  }
  if ('summary_json' in object) {
    return hasBlockText(object.summary_json);
  }
  return Object.entries(object)
    .filter(([key]) => key !== 'MeetingName' && key !== '_section_order')
    .some(([, section]) => hasLegacySectionText(section));
}

const SummaryMetadata = z.object({
  MeetingName: z.string().optional(),
  reasoning_stripped: z.boolean().optional(),
  normalization_fallback: z.boolean().optional(),
}).passthrough();

export function readSummaryMetadata(value: unknown) {
  const parsed = SummaryMetadata.safeParse(value);
  return parsed.success
    ? {
        meetingName: parsed.data.MeetingName ?? null,
        reasoningStripped: parsed.data.reasoning_stripped === true,
        normalizationFallback: parsed.data.normalization_fallback === true,
      }
    : {
        meetingName: null,
        reasoningStripped: false,
        normalizationFallback: false,
      };
}

export function parseSummaryContent(value: unknown): MeetingSummary | null {
  if (typeof value === 'string') {
    try {
      value = JSON.parse(value);
    } catch {
      return null;
    }
  }
  return hasVisibleSummaryContent(value) ? value : null;
}

/** 旧摘要转 markdown；仅 BlockNote 区块（summary_json）时返回 null，由渲染器直接渲染。 */
export function summaryToMarkdown(value: unknown): string | null {
  const summary = parseSummaryContent(value);
  if (!summary) {
    return null;
  }
  if (typeof summary.markdown === 'string' && summary.markdown.trim()) {
    return summary.markdown;
  }

  const sections = Object.entries(summary)
    .filter(([key]) => !['markdown', 'summary_json', '_section_order', 'MeetingName'].includes(key))
    .map(([, section]) => {
      const legacy = parseSummaryObject(section);
      if (typeof legacy?.title !== 'string' || !Array.isArray(legacy.blocks)) {
        return '';
      }
      const blocks = legacy.blocks
        .map((block) => parseSummaryObject(block)?.content)
        .filter((content): content is string => typeof content === 'string' && content.trim().length > 0)
        .map((content) => `- ${content}`)
        .join('\n');
      return blocks ? `## ${legacy.title}\n\n${blocks}` : '';
    })
    .filter(Boolean)
    .join('\n\n');

  return sections || null;
}
