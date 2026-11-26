"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { speakerText } from '@/components/Transcript/SpeakerBadge';
import { RenameSpeakerDialog } from '@/components/Transcript/RenameSpeakerDialog';
import { Button } from '@/components/ui/button';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { invoke } from '@tauri-apps/api/core';
import { useEffect, useMemo, useState } from 'react';
import { useDiarization } from '@/hooks/useDiarization';

interface TranscriptPanelProps {
  transcripts: Transcript[];
  customPrompt: string;
  onPromptChange: (value: string) => void;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  isRecording: boolean;
  disableAutoScroll?: boolean;

  // Optional pagination props (when using virtualization)
  usePagination?: boolean;
  segments?: TranscriptSegmentData[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;

  // Retranscription props
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
}

export function TranscriptPanel({
  transcripts,
  customPrompt,
  onPromptChange,
  onCopyTranscript,
  onOpenMeetingFolder,
  isRecording,
  disableAutoScroll = false,
  usePagination = false,
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
}: TranscriptPanelProps) {
  // Convert transcripts to segments if pagination is not used but we want virtualization
  const convertedSegments = useMemo(() => {
    if (usePagination && segments) {
      return segments;
    }
    // Convert transcripts to segments for virtualization
    return transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      endTime: t.audio_end_time,
      text: t.text,
      confidence: t.confidence,
      speaker: t.speaker ?? null,
    }));
  }, [transcripts, usePagination, segments]);

  const [speakerNames, setSpeakerNames] = useState<Record<string, string>>({});
  const [speakerFilter, setSpeakerFilter] = useState<string>('all');
  const [renameTarget, setRenameTarget] = useState<{ label: string; current: string } | null>(null);
  const diarization = useDiarization(meetingId ?? '');

  // refreshKey 递增：分离完成或重命名后标签已写库，重拉显示名与逐字稿
  useEffect(() => {
    if (!meetingId) return;
    let cancelled = false;
    invoke<{ label: string; displayName: string | null }[]>('api_get_meeting_speakers', { meetingId })
      .then((rows) => {
        if (cancelled) return;
        const map: Record<string, string> = {};
        rows.forEach((row) => { if (row.displayName) map[row.label] = row.displayName; });
        setSpeakerNames(map);
      })
      .catch((error) => console.error('加载说话人失败:', error));
    if (diarization.refreshKey > 0) {
      onRefetchTranscripts?.().catch((error) => console.error('刷新逐字稿失败:', error));
    }
    return () => { cancelled = true; };
  }, [meetingId, diarization.refreshKey]);

  const speakerLabels = useMemo(
    () => Array.from(new Set(convertedSegments.map((s) => s.speaker).filter(Boolean) as string[])).sort(),
    [convertedSegments],
  );
  const hasSpeakers = speakerLabels.length > 0;
  // 重新转写后标签可能变化：失效的筛选回落为「全部」，避免下拉显示全部而列表为空
  const activeFilter = speakerLabels.includes(speakerFilter) ? speakerFilter : 'all';
  useEffect(() => {
    if (!speakerLabels.includes(speakerFilter)) setSpeakerFilter('all');
  }, [speakerLabels, speakerFilter]);

  const visibleSegments = useMemo(
    () => (activeFilter === 'all' ? convertedSegments : convertedSegments.filter((s) => s.speaker === activeFilter)),
    [convertedSegments, activeFilter],
  );

  return (
    <div className="flex h-full min-w-0 w-full bg-white flex-col relative @container">
      {/* Title area */}
      <div className="p-4 border-b border-gray-200 flex items-center justify-center gap-2">
        <TranscriptButtonGroup
          transcriptCount={usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0)}
          onCopyTranscript={onCopyTranscript}
          onOpenMeetingFolder={onOpenMeetingFolder}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onRefetchTranscripts={onRefetchTranscripts}
        />
        {diarization.status === 'processing' && (
          <>
            <span className="text-sm text-gray-600">正在区分说话人… {diarization.progress}%</span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void diarization.cancel().catch((error) => console.error('取消说话人分离失败:', error))}
            >
              停止
            </Button>
          </>
        )}
        {diarization.unavailable && (
          <span className="text-sm text-amber-700">未安装说话人模型，无法区分说话人（可在设置中下载）</span>
        )}
        {hasSpeakers && (
          <select
            value={activeFilter}
            onChange={(e) => setSpeakerFilter(e.target.value)}
            className="rounded border border-gray-200 px-2 py-1 text-sm"
            aria-label="按说话人筛选"
          >
            <option value="all">全部说话人</option>
            {speakerLabels.map((label) => (
              <option key={label} value={label}>{speakerText(label, speakerNames[label])}</option>
            ))}
          </select>
        )}
      </div>

      {/* Transcript content - use virtualized view for better performance */}
      <div className="flex-1 overflow-hidden pb-4">
        <VirtualizedTranscriptView
          segments={visibleSegments}
          isRecording={isRecording}
          isPaused={false}
          isProcessing={false}
          isStopping={false}
          enableStreaming={false}
          showConfidence={true}
          disableAutoScroll={disableAutoScroll}
          hasMore={hasMore}
          isLoadingMore={isLoadingMore}
          totalCount={totalCount}
          loadedCount={loadedCount}
          onLoadMore={onLoadMore}
          names={speakerNames}
          onSpeakerClick={(label) => setRenameTarget({ label, current: speakerNames[label] ?? '' })}
        />
      </div>

      {meetingId && renameTarget && (
        <RenameSpeakerDialog
          meetingId={meetingId}
          label={renameTarget.label}
          current={renameTarget.current}
          onClose={() => setRenameTarget(null)}
          onSaved={() => diarization.bump()}
        />
      )}

      {/* Custom prompt input at bottom of transcript section */}
      {!isRecording && convertedSegments.length > 0 && (
        <div className="p-1 border-t border-gray-200">
          <textarea
            placeholder="为 AI 纪要补充上下文，例如参会人员、会议概览、目标等…"
            className="w-full px-3 py-2 border border-gray-200 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 bg-white shadow-sm min-h-[80px] resize-y"
            value={customPrompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />
        </div>
      )}
    </div>
  );
}
