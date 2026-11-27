'use client';

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Eye, FileCode, FileSpreadsheet, FileText, Play, Square } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
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
  canGenerate: boolean;
  onGenerate: () => void | Promise<void>;
  onCancel: () => void;
}

export function MinutesToolbar({
  meetingId,
  status,
  progress,
  error,
  hasMinutes,
  canGenerate,
  onGenerate,
  onCancel,
}: MinutesToolbarProps) {
  // 各导出/预览各自独立的 loading 状态，互不影响。
  const [exporting, setExporting] = useState({ excel: false, markdown: false, html: false });
  const [preview, setPreview] = useState({ open: false, loading: false, html: '' });
  const processing = status === 'processing';

  const handleExport = async (kind: 'excel' | 'markdown' | 'html') => {
    const command = {
      excel: 'api_export_minutes_excel',
      markdown: 'api_export_minutes_markdown',
      html: 'api_export_minutes_html',
    }[kind];
    const label = { excel: 'Excel', markdown: 'Markdown', html: 'HTML' }[kind];
    setExporting((state) => ({ ...state, [kind]: true }));
    try {
      const path = await invoke<string>(command, { meetingId });
      toast.success(`${label} 已导出`, {
        description: path,
        action: {
          label: '打开数据目录',
          onClick: () => {
            invoke('open_database_folder').catch(() => toast.error('打开数据目录失败'));
          },
        },
      });
    } catch (cause) {
      toast.error(`导出 ${label} 失败：${String(cause)}`);
    } finally {
      setExporting((state) => ({ ...state, [kind]: false }));
    }
  };

  const handlePreview = async () => {
    setPreview({ open: true, loading: true, html: '' });
    try {
      const html = await invoke<string>('api_get_minutes_html', { meetingId });
      setPreview({ open: true, loading: false, html });
    } catch (cause) {
      setPreview({ open: false, loading: false, html: '' });
      toast.error(`预览失败：${String(cause)}`);
    }
  };

  // 关闭对话框时清空 html，避免下次打开闪现旧内容。
  const handlePreviewOpenChange = (open: boolean) => {
    setPreview((state) => (open ? { ...state, open } : { open: false, loading: false, html: '' }));
  };

  return (
    <div className="border-b border-gray-200 p-4">
      <div className="flex w-full min-w-0 flex-wrap items-center justify-center gap-2">
        {processing ? (
          <Button variant="red" size="sm" onClick={onCancel}>
            <Square size={16} />
            停止
          </Button>
        ) : canGenerate ? (
          <Button variant="blue" size="sm" onClick={() => void onGenerate()}>
            <Play size={16} />
            生成纪要
          </Button>
        ) : null}
        <Button
          variant="outline"
          size="sm"
          onClick={() => void handleExport('excel')}
          disabled={!hasMinutes || exporting.excel}
        >
          <FileSpreadsheet size={16} />
          {exporting.excel ? '导出中…' : '导出 Excel'}
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => void handleExport('markdown')}
          disabled={!hasMinutes || exporting.markdown}
        >
          <FileText size={16} />
          {exporting.markdown ? '导出中…' : '导出 Markdown'}
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => void handleExport('html')}
          disabled={!hasMinutes || exporting.html}
        >
          <FileCode size={16} />
          {exporting.html ? '导出中…' : '导出 HTML'}
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => void handlePreview()}
          disabled={!hasMinutes || preview.loading}
        >
          <Eye size={16} />
          {preview.loading ? '预览中…' : '预览'}
        </Button>
      </div>

      {processing && (
        <div className="mt-3 space-y-1">
          <Progress value={progress.percent} />
          <p aria-live="polite" className="text-center text-xs text-gray-500">
            {progress.stage ? `${STAGE_LABELS[progress.stage] ?? progress.stage} ${progress.percent}%` : '准备中…'}
          </p>
        </div>
      )}

      {status === 'error' && error && (
        <p role="alert" className="mt-3 text-center text-sm text-red-600">
          {error}
        </p>
      )}

      <Dialog open={preview.open} onOpenChange={handlePreviewOpenChange}>
        <DialogContent aria-describedby={undefined} className="max-w-4xl">
          <DialogHeader>
            <DialogTitle>纪要预览</DialogTitle>
          </DialogHeader>
          {preview.loading ? (
            <p className="py-8 text-center text-sm text-gray-500">加载中…</p>
          ) : (
            <iframe
              srcDoc={preview.html}
              sandbox=""
              className="h-[70vh] w-full rounded border"
              title="纪要预览"
            />
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
