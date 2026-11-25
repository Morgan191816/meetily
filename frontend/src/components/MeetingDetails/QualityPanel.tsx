'use client';

import { BlockNoteSummaryView } from '@/components/AISummary/BlockNoteSummaryView';

interface QualityPanelProps {
  quality: string | null;
}

export function QualityPanel({ quality }: QualityPanelProps) {
  if (!quality) {
    return (
      <div className="flex h-full w-full min-w-0 flex-1 items-center justify-center bg-white p-6">
        <p className="text-gray-600">尚未生成质检报告</p>
      </div>
    );
  }

  return (
    <div className="h-full w-full min-w-0 flex-1 overflow-y-auto overflow-x-auto bg-white">
      <div className="w-full p-6">
        <BlockNoteSummaryView summaryData={{ markdown: quality }} editable={false} />
      </div>
    </div>
  );
}
