import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export type DiarizationStatus = 'idle' | 'processing' | 'done' | 'unavailable';

interface DiarizationProgressEvent {
  meetingId: string;
  stage: string;
  percent: number;
}

interface MeetingEvent {
  meetingId: string;
}

/** 主动查询结果只在空闲时生效，避免覆盖正在进行的分离。 */
export function availabilityAfterCheck(state: string, current: DiarizationStatus): DiarizationStatus {
  return state === 'missing' && current === 'idle' ? 'unavailable' : current;
}

/**
 * 说话人分离状态：进度与结果由后端事件驱动；`refreshKey` 递增表示标签已写库，
 * 逐字稿面板据此重拉说话人名与逐字稿。
 */
export function useDiarization(meetingId: string) {
  const [status, setStatus] = useState<DiarizationStatus>('idle');
  const [progress, setProgress] = useState(0);
  const [refreshKey, setRefreshKey] = useState(0);
  // 取消后仍可能收到已排队的进度事件；新一轮从 decoding 开始，用它复位
  const cancelledRef = useRef(false);

  const bump = useCallback(() => setRefreshKey((key) => key + 1), []);

  useEffect(() => {
    if (!meetingId) return;
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];

    const attach = async () => {
      const unlistenProgress = await listen<DiarizationProgressEvent>('diarization-progress', (event) => {
        if (event.payload.meetingId !== meetingId) return;
        if (event.payload.stage === 'decoding') cancelledRef.current = false;
        if (cancelledRef.current) return;
        setStatus('processing');
        setProgress(event.payload.percent);
      });
      if (disposed) {
        unlistenProgress();
        return;
      }
      unlisteners.push(unlistenProgress);

      const unlistenUpdated = await listen<MeetingEvent>('speakers-updated', (event) => {
        if (event.payload.meetingId !== meetingId) return;
        cancelledRef.current = false;
        setStatus('done');
        setProgress(100);
        bump();
      });
      if (disposed) {
        unlistenUpdated();
        unlisteners.forEach((unlisten) => unlisten());
        return;
      }
      unlisteners.push(unlistenUpdated);

      const unlistenUnavailable = await listen<MeetingEvent>('diarization-unavailable', (event) => {
        if (event.payload.meetingId !== meetingId) return;
        setStatus('unavailable');
      });
      if (disposed) {
        unlistenUnavailable();
        unlisteners.forEach((unlisten) => unlisten());
        return;
      }
      unlisteners.push(unlistenUnavailable);
    };

    attach().catch((error) => console.error('注册说话人事件失败:', error));

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [meetingId, bump]);

  // 页面加载时主动查询，弥补导入时事件早于监听器挂载的丢失
  const checkAvailability = useCallback(async () => {
    if (!meetingId) return;
    try {
      const status = await invoke<{ state: string }>('diarization_get_status');
      setStatus((prev) => availabilityAfterCheck(status.state, prev));
    } catch (error) {
      console.error('查询说话人模型状态失败:', error);
    }
  }, [meetingId]);

  const cancel = useCallback(async () => {
    cancelledRef.current = true;
    try {
      await invoke<boolean>('api_cancel_diarization', { meetingId });
    } catch (error) {
      cancelledRef.current = false;
      throw error;
    }
    setStatus('idle');
    setProgress(0);
  }, [meetingId]);

  return { status, progress, unavailable: status === 'unavailable', refreshKey, bump, cancel, checkAvailability };
}
