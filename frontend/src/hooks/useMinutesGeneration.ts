import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export type MinutesStage =
  | 'correcting'
  | 'summarizing'
  | 'extracting'
  | 'tracing'
  | 'validating'
  | 'rendering'
  | 'done';

export type MinutesStatus = 'idle' | 'processing' | 'completed' | 'error';

export interface MinutesProgress {
  stage: MinutesStage | null;
  percent: number;
}

/** `api_generate_minutes` / `api_get_minutes` 的返回（Rust 侧 camelCase）。 */
export interface MinutesRecord {
  meetingId: string;
  markdown: string;
  qualityMarkdown: string;
  tasks: unknown[];
  traceabilityPass: boolean;
  validationPass: boolean;
  createdAt: string;
  updatedAt: string;
}

interface MinutesProgressEvent {
  meetingId: string;
  stage: MinutesStage;
  percent: number;
}

interface MinutesErrorEvent {
  meetingId: string;
  error: string;
}

const IDLE_PROGRESS: MinutesProgress = { stage: null, percent: 0 };

/**
 * 纪要生成状态：`generate` 触发后端管线，进度与失败由 `minutes-progress` /
 * `minutes-error` 事件驱动；取消只复位状态，不清空已显示的旧纪要。
 */
export function useMinutesGeneration(meetingId: string) {
  const [status, setStatus] = useState<MinutesStatus>('idle');
  const [progress, setProgress] = useState<MinutesProgress>(IDLE_PROGRESS);
  const [minutes, setMinutes] = useState<string | null>(null);
  const [quality, setQuality] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // 取消后 generate 的 Promise 仍会以「已取消」拒绝，用它区分取消与真实失败
  const cancelledRef = useRef(false);

  const load = useCallback(async () => {
    const record = await invoke<MinutesRecord | null>('api_get_minutes', { meetingId });
    setMinutes(record?.markdown ?? null);
    setQuality(record?.qualityMarkdown ?? null);
  }, [meetingId]);

  // 打开会议时先展示已生成的纪要
  useEffect(() => {
    load().catch((cause) => console.error('加载会议纪要失败:', cause));
  }, [load]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];

    const attach = async () => {
      const unlistenProgress = await listen<MinutesProgressEvent>('minutes-progress', (event) => {
        if (event.payload.meetingId !== meetingId) return;
        // 取消后迟到的阶段事件不得把状态改回 processing；done 仍要收，管线可能已成功
        if (cancelledRef.current && event.payload.stage !== 'done') return;
        setProgress({ stage: event.payload.stage, percent: event.payload.percent });
        if (event.payload.stage === 'done') {
          setStatus('completed');
          load().catch((cause) => console.error('加载会议纪要失败:', cause));
        } else {
          setStatus('processing');
        }
      });
      if (disposed) {
        unlistenProgress();
        return;
      }
      unlisteners.push(unlistenProgress);

      const unlistenError = await listen<MinutesErrorEvent>('minutes-error', (event) => {
        if (event.payload.meetingId !== meetingId) return;
        if (cancelledRef.current) return;
        setStatus('error');
        setError(event.payload.error);
      });
      if (disposed) {
        unlistenError();
        unlisteners.forEach((unlisten) => unlisten());
        return;
      }
      unlisteners.push(unlistenError);
    };

    attach().catch((cause) => console.error('注册纪要事件监听失败:', cause));

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [meetingId, load]);

  const generate = useCallback(async () => {
    cancelledRef.current = false;
    setStatus('processing');
    setError(null);
    setProgress(IDLE_PROGRESS);
    try {
      const record = await invoke<MinutesRecord>('api_generate_minutes', { meetingId });
      if (cancelledRef.current) return;
      setMinutes(record.markdown);
      setQuality(record.qualityMarkdown);
      setStatus('completed');
      setProgress({ stage: 'done', percent: 100 });
    } catch (cause) {
      if (cancelledRef.current) return;
      setStatus('error');
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [meetingId]);

  const cancel = useCallback(async () => {
    cancelledRef.current = true;
    try {
      const cancelled = await invoke<boolean>('api_cancel_minutes', { meetingId });
      if (!cancelled) {
        // 后端没有在途任务：让 generate 的 Promise 自己收尾
        cancelledRef.current = false;
        return false;
      }
    } catch (cause) {
      cancelledRef.current = false;
      throw cause;
    }
    setStatus('idle');
    setProgress(IDLE_PROGRESS);
    setError(null);
    return true;
  }, [meetingId]);

  return { status, progress, generate, cancel, minutes, quality, error };
}
