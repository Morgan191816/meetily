'use client';

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { FileSpreadsheet, Play, Square } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import type { MinutesProgress, MinutesStage, MinutesStatus } from '@/hooks/useMinutesGeneration';

const STAGE_LABELS: Record<MinutesStage, string> = {
  correcting: '术语纠正',
  summarizing: '生成纪要',
  extracting: '提取行动项',
  tracing: '溯源核对',
  validating: '质量校验',
  rendering: '渲染输出',
  done: '完成',
};

interface MinutesToolbarProps {
  meetingId: string;
  status: MinutesStatus;
  progress: MinutesProgress;
  error: string | null;
  hasMinutes: boolean;
  onGenerate: () => void | Promise<void>;
  onCancel: () => void;
}

export function MinutesToolbar({
  meetingId,
  status,
  progress,
  error,
  hasMinutes,
  onGenerate,
  onCancel,
}: MinutesToolbarProps) {
  const [exporting, setExporting] = useState(false);
  const processing = status === 'processing';

  const handleExport = async () => {
    setExporting(true);
    try {
      const path = await invoke<string>('api_export_minutes_excel', { meetingId });
      toast.success('Excel 已导出', {
        description: path,
        action: {
          label: '打开文件夹',
          onClick: () => {
            void invoke('open_database_folder');
          },
        },
      });
    } catch (cause) {
      toast.error(`导出 Excel 失败：${String(cause)}`);
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="border-b border-gray-200 p-4">
      <div className="flex w-full min-w-0 flex-wrap items-center justify-center gap-2">
        {processing ? (
          <Button variant="red" size="sm" onClick={onCancel}>
            <Square size={16} />
            停止
          </Button>
        ) : (
          <Button variant="blue" size="sm" onClick={() => void onGenerate()}>
            <Play size={16} />
            生成纪要
          </Button>
        )}
        <Button
          variant="outline"
          size="sm"
          onClick={() => void handleExport()}
          disabled={!hasMinutes || exporting}
        >
          <FileSpreadsheet size={16} />
          {exporting ? '导出中…' : '导出 Excel'}
        </Button>
      </div>

      {processing && (
        <div className="mt-3 space-y-1">
          <Progress value={progress.percent} />
          <p className="text-center text-xs text-gray-500">
            {STAGE_LABELS[progress.stage ?? 'correcting']} {progress.percent}%
          </p>
        </div>
      )}

      {status === 'error' && error && (
        <p role="alert" className="mt-3 text-center text-sm text-red-600">
          {error}
        </p>
      )}
    </div>
  );
}
