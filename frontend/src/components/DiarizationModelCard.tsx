import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

type CardState = 'loading' | 'missing' | 'downloading' | 'available' | 'statusError';

const MODEL_SIZE_LABEL = '约 34 MB';

interface Status {
  state: 'missing' | 'downloading' | 'available';
  progress?: number | null;
  path: string;
}

/**
 * 说话人分离模型卡：状态检查、下载、取消。
 * 两个模型文件合并为一条进度（segmentation 0–40%、embedding 40–100%），
 * 完成以 diarization-download-complete 或重新查询为准。
 */
export function DiarizationModelCard({ className = '' }: { className?: string }) {
  const [state, setState] = useState<CardState>('loading');
  const [progress, setProgress] = useState(0);
  const [cancelling, setCancelling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const status = await invoke<Status>('diarization_get_status');
      setState(status.state);
      setProgress(status.progress ?? 0);
    } catch (err) {
      // 状态读取失败不是「未下载」：保留错误态，别把 IPC/目录故障伪装成可下载。
      console.error('读取说话人模型状态失败:', err);
      setState('statusError');
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let unlistenProgress: (() => void) | null = null;
    let unlistenComplete: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;

    const setup = async () => {
      unlistenProgress = await listen<{ progress: number }>('diarization-download-progress', (event) => {
        setState('downloading');
        setProgress(event.payload.progress);
      });
      unlistenComplete = await listen('diarization-download-complete', () => {
        setCancelling(false);
        void refresh();
      });
      unlistenError = await listen<{ error: string }>('diarization-download-error', (event) => {
        setCancelling(false);
        setError(event.payload.error);
        void refresh();
      });
    };
    void setup();

    return () => {
      unlistenProgress?.();
      unlistenComplete?.();
      unlistenError?.();
    };
  }, [refresh]);

  const startDownload = async () => {
    setError(null);
    setState('downloading');
    setProgress(0);
    try {
      await invoke('diarization_download_models');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCancelling(false);
      await refresh();
    }
  };

  const cancelDownload = async () => {
    setCancelling(true);
    try {
      await invoke('diarization_cancel_download');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setCancelling(false);
    }
  };

  return (
    <div className={`rounded-lg border-2 border-gray-200 bg-white p-3 ${className}`}>
      <div className="flex items-start justify-between">
        <div className="flex-1">
          <h3 className="font-semibold text-gray-900">说话人分离模型</h3>
          <p className="text-sm text-gray-500 mt-1">
            会议结束后区分不同说话人，首次使用需下载（{MODEL_SIZE_LABEL}）
          </p>
        </div>

        <div className="ml-4 flex items-center gap-2">
          {state === 'loading' && <span className="text-xs text-gray-500">检查中…</span>}

          {state === 'available' && (
            <div className="flex items-center gap-1.5 text-green-600">
              <div className="w-2 h-2 bg-green-500 rounded-full"></div>
              <span className="text-xs font-medium">已就绪</span>
            </div>
          )}

          {state === 'statusError' && (
            <div className="flex items-center gap-2">
              <span className="text-xs text-red-600">读取模型状态失败，请重试</span>
              <button
                onClick={() => {
                  setState('loading');
                  void refresh();
                }}
                className="text-xs text-gray-600 hover:text-blue-600 font-medium transition-colors px-2 py-1 rounded hover:bg-blue-50"
              >
                重试
              </button>
            </div>
          )}

          {state === 'missing' && (
            <button
              onClick={() => void startDownload()}
              className="bg-blue-600 text-white px-3 py-1.5 rounded-md text-sm font-medium hover:bg-blue-700 transition-colors"
            >
              {`下载模型（${MODEL_SIZE_LABEL}）`}
            </button>
          )}
        </div>
      </div>

      {state === 'downloading' && (
        <div className="mt-3 pt-3 border-t border-gray-200">
          <div className="flex items-center justify-between mb-2">
            <span className="text-sm font-medium text-blue-600">
              {cancelling ? '正在取消…' : `正在下载… ${Math.round(progress)}%`}
            </span>
            {!cancelling && (
              <button
                onClick={() => void cancelDownload()}
                className="text-xs text-gray-600 hover:text-red-600 font-medium transition-colors px-2 py-1 rounded hover:bg-red-50"
                title="取消下载"
              >
                取消
              </button>
            )}
          </div>
          <div className="w-full h-2 bg-gray-200 rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-blue-500 to-blue-600 rounded-full transition-all"
              style={{ width: `${progress}%` }}
            />
          </div>
        </div>
      )}

      {error && <p className="text-xs text-red-600 mt-2">下载失败：{error}</p>}
    </div>
  );
}
