'use client';

import { BlockNoteSummaryView } from '@/components/AISummary/BlockNoteSummaryView';
import { Button } from '@/components/ui/button';
import type { MinutesProgress, MinutesStatus } from '@/hooks/useMinutesGeneration';
import { MinutesToolbar } from './MinutesToolbar';

interface MinutesPanelProps {
  meetingId: string;
  status: MinutesStatus;
  progress: MinutesProgress;
  minutes: string | null;
  error: string | null;
  onGenerate: () => void | Promise<void>;
  onCancel: () => void;
}

export function MinutesPanel({
  meetingId,
  status,
  progress,
  minutes,
  error,
  onGenerate,
  onCancel,
}: MinutesPanelProps) {
  return (
    <div className="flex h-full w-full min-w-0 flex-1 flex-col overflow-hidden bg-white">
      <MinutesToolbar
        meetingId={meetingId}
        status={status}
        progress={progress}
        error={error}
        hasMinutes={Boolean(minutes)}
        onGenerate={onGenerate}
        onCancel={onCancel}
      />

      {minutes ? (
        <div className="min-h-0 flex-1 overflow-y-auto overflow-x-auto">
          <div className="w-full p-6">
            <BlockNoteSummaryView summaryData={{ markdown: minutes }} />
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
