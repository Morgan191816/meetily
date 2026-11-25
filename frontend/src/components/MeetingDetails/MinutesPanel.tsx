'use client';

import { BlockNoteSummaryView } from '@/components/AISummary/BlockNoteSummaryView';
import { Button } from '@/components/ui/button';
import type { MinutesProgress, MinutesStatus } from '@/hooks/useMinutesGeneration';
import { parseSummaryContent, summaryToMarkdown } from '@/lib/summary-content';
import type { MeetingSummary } from '@/types';
import { MinutesToolbar } from './MinutesToolbar';

interface MinutesPanelProps {
  meetingId: string;
  status: MinutesStatus;
  progress: MinutesProgress;
  minutes: string | null;
  error: string | null;
  legacySummary?: MeetingSummary | null;
  onGenerate: () => void | Promise<void>;
  onCancel: () => void;
}

export function MinutesPanel({
  meetingId,
  status,
  progress,
  minutes,
  error,
  legacySummary = null,
  onGenerate,
  onCancel,
}: MinutesPanelProps) {
  // 旧版摘要只读回退：minutes 为空时展示 summary_processes.result 中的历史纪要。
  const legacy = minutes ? null : parseSummaryContent(legacySummary);
  const legacyMarkdown = legacy ? summaryToMarkdown(legacy) : null;
  const legacyData = legacyMarkdown ? { markdown: legacyMarkdown } : legacy;

  return (
    <div className="flex h-full w-full min-w-0 flex-1 flex-col overflow-hidden bg-white">
      <MinutesToolbar
        meetingId={meetingId}
        status={status}
        progress={progress}
        error={error}
        hasMinutes={Boolean(minutes)}
        canGenerate={Boolean(minutes) || Boolean(legacyData)}
        onGenerate={onGenerate}
        onCancel={onCancel}
      />

      {minutes ? (
        <div className="min-h-0 flex-1 overflow-y-auto overflow-x-auto">
          <div className="w-full p-6">
            <BlockNoteSummaryView summaryData={{ markdown: minutes }} editable={false} />
          </div>
        </div>
      ) : legacyData ? (
        <div className="flex min-h-0 flex-1 flex-col">
          <p role="status" className="border-b border-amber-200 bg-amber-50 px-6 py-2 text-sm text-amber-800">
            此纪要由旧版生成，重新生成将使用新版纪要
          </p>
          <div className="min-h-0 flex-1 overflow-y-auto overflow-x-auto">
            <div className="w-full p-6">
              <BlockNoteSummaryView summaryData={legacyData} editable={false} />
            </div>
          </div>
        </div>
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
          <p className="text-gray-600">尚未生成会议纪要</p>
          <p className="text-sm text-gray-500">
            点击「生成纪要」，从逐字稿生成纪要、行动项与质检报告。
          </p>
          {status !== 'processing' && (
            <Button variant="blue" size="sm" onClick={() => void onGenerate()}>
              生成纪要
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
