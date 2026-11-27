'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ChevronDown, Download, Eye, FileCode, FileSpreadsheet, FileText, Play, Square } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Progress } from '@/components/ui/progress';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import type {
  MinutesProgress,
  MinutesStage,
  MinutesStatus,
  MinutesTemplateOption,
} from '@/hooks/useMinutesGeneration';

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
  /** 当前纪要所用模板；无纪要时为 null。 */
  templateId: string | null;
  selectedTemplate: string | null;
  onSelectTemplate: (templateId: string) => void;
  onGenerate: (templateId?: string) => void | Promise<void>;
  onCancel: () => void;
}

export function MinutesToolbar({
  meetingId,
  status,
  progress,
  error,
  hasMinutes,
  canGenerate,
  templateId,
  selectedTemplate,
  onSelectTemplate,
  onGenerate,
  onCancel,
}: MinutesToolbarProps) {
  // 各导出/预览各自独立的 loading 状态，互不影响。
  const [exporting, setExporting] = useState({ excel: false, markdown: false, html: false });
  const [preview, setPreview] = useState({ open: false, loading: false, html: '' });
  // 模板列表加载失败时保持 null：下拉隐藏，生成走后端默认模板
  const [templates, setTemplates] = useState<MinutesTemplateOption[] | null>(null);
  const [defaultTemplateId, setDefaultTemplateId] = useState<string | null>(null);
  const processing = status === 'processing';

  useEffect(() => {
    let disposed = false;
    Promise.all([
      invoke<MinutesTemplateOption[]>('api_list_minutes_templates'),
      invoke<string>('api_get_default_minutes_template'),
    ])
      .then(([list, fallback]) => {
        if (disposed) return;
        setTemplates(list);
        setDefaultTemplateId(fallback);
      })
      .catch((cause) => console.error('加载纪要模板失败:', cause));
    return () => {
      disposed = true;
    };
  }, []);

  // 初始选中：当前纪要所用模板 ?? 全局默认；纪要异步读回后同步一次
  useEffect(() => {
    if (!templates || !defaultTemplateId) return;
    onSelectTemplate(templateId ?? defaultTemplateId);
  }, [templates, defaultTemplateId, templateId, onSelectTemplate]);

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
        {templates && selectedTemplate && (
          <div className="flex items-center gap-2">
            <Select value={selectedTemplate} onValueChange={onSelectTemplate} disabled={processing}>
              <SelectTrigger className="h-8 w-40" aria-label="纪要模板">
                <SelectValue placeholder="选择纪要模板" />
              </SelectTrigger>
              <SelectContent>
                {templates.map((template) => (
                  <SelectItem key={template.id} value={template.id}>
                    {template.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {hasMinutes && <span className="text-xs text-gray-500">重新生成会覆盖当前纪要</span>}
          </div>
        )}
        {processing ? (
          <Button variant="red" size="sm" onClick={onCancel}>
            <Square size={16} />
            停止
          </Button>
        ) : canGenerate ? (
          <Button variant="blue" size="sm" onClick={() => void onGenerate(selectedTemplate ?? undefined)}>
            <Play size={16} />
            生成纪要
          </Button>
        ) : null}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="outline"
              size="sm"
              disabled={!hasMinutes || exporting.excel || exporting.markdown || exporting.html}
            >
              <Download size={16} />
              导出
              <ChevronDown size={14} className="ml-1" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem disabled={exporting.excel} onSelect={() => void handleExport('excel')}>
              <FileSpreadsheet size={16} className="mr-2" />
              {exporting.excel ? '正在导出 Excel…' : '导出 Excel'}
            </DropdownMenuItem>
            <DropdownMenuItem disabled={exporting.markdown} onSelect={() => void handleExport('markdown')}>
              <FileText size={16} className="mr-2" />
              {exporting.markdown ? '正在导出 Markdown…' : '导出 Markdown'}
            </DropdownMenuItem>
            <DropdownMenuItem disabled={exporting.html} onSelect={() => void handleExport('html')}>
              <FileCode size={16} className="mr-2" />
              {exporting.html ? '正在导出 HTML…' : '导出 HTML'}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem disabled={preview.loading} onSelect={() => void handlePreview()}>
              <Eye size={16} className="mr-2" />
              {preview.loading ? '正在生成预览…' : '预览 HTML'}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
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
