import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export type DiarizationStatus = 'idle' | 'processing' | 'done' | 'unavailable';

interface DiarizationProgressEvent {
  meetingId: string;
  stage: string;
  percent: number;
}

interface DiarizationErrorEvent {
  meetingId: string;
  error: string;
}

interface MeetingEvent {
  meetingId: string;
  labels?: number;
}

/** 主动查询结果只在空闲时生效，避免覆盖正在进行的分离。 */
export function availabilityAfterCheck(state: string, current: DiarizationStatus): DiarizationStatus {
  return state === 'missing' && current === 'idle' ? 'unavailable' : current;
}

/** 失败必须把状态从 processing 复位，否则进度条永久停在半路。 */
export function failureAfterRun(error: string): {
  status: DiarizationStatus;
  progress: number;
  error: string;
} {
  return { status: 'idle', progress: 0, error };
}

/** 取消不是失败：与自动路径一致，错误串含「已取消」时不提示。 */
export function errorAfterCancel(error: string): string | null {
  return error.includes('已取消') ? null : error;
}

/**
 * 说话人分离状态：进度与结果由后端事件驱动。
 * `nameKey` 递增表示只需重拉显示名，`labelKey` 递增表示标签已写库、逐字稿需重拉。
 */
export function useDiarization(meetingId: string) {
  const [status, setStatus] = useState<DiarizationStatus>('idle');
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [empty, setEmpty] = useState(false);
  const [nameKey, setNameKey] = useState(0);
  const [labelKey, setLabelKey] = useState(0);
  // 取消后仍可能收到已排队的进度事件；新一轮从 decoding 开始，用它复位
  const cancelledRef = useRef(false);

  const bumpNames = useCallback(() => setNameKey((key) => key + 1), []);
  const bumpLabels = useCallback(() => setLabelKey((key) => key + 1), []);

  useEffect(() => {
    if (!meetingId) return;
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];

    const attach = async () => {
      const unlistenProgress = await listen<DiarizationProgressEvent>('diarization-progress', (event) => {
        if (event.payload.meetingId !== meetingId) return;
        if (event.payload.stage === 'decoding') {
          cancelledRef.current = false;
          setError(null);
          setEmpty(false);
        }
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
        setError(null);
        setEmpty((event.payload.labels ?? 1) === 0);
        setStatus('done');
        setProgress(100);
        bumpLabels();
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

      const unlistenError = await listen<DiarizationErrorEvent>('diarization-error', (event) => {
        if (event.payload.meetingId !== meetingId) return;
        cancelledRef.current = false;
        const failure = failureAfterRun(event.payload.error);
        setStatus(failure.status);
        setProgress(failure.progress);
        setError(failure.error);
      });
      if (disposed) {
        unlistenError();
        unlisteners.forEach((unlisten) => unlisten());
        return;
      }
      unlisteners.push(unlistenError);
    };

    attach().catch((error) => console.error('注册说话人事件失败:', error));

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [meetingId, bumpLabels]);

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

  // 手动重新分离：进度事件驱动状态，失败由命令返回值兜底
  const rerun = useCallback(async () => {
    if (!meetingId) return;
    cancelledRef.current = false;
    setStatus('processing');
    setProgress(0);
    setError(null);
    setEmpty(false);
    try {
      await invoke<number>('api_run_diarization', { meetingId });
    } catch (raw) {
      const message = String(raw);
      const failure = failureAfterRun(message);
      setStatus(failure.status);
      setProgress(failure.progress);
      setError(errorAfterCancel(message));
    }
  }, [meetingId]);

  return {
    status,
    progress,
    unavailable: status === 'unavailable',
    error,
    empty,
    nameKey,
    labelKey,
    bumpNames,
    bumpLabels,
    rerun,
    cancel,
    checkAvailability,
  };
}
