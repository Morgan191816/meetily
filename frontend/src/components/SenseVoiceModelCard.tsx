import { useCallback, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { SenseVoiceAPI, SenseVoiceModelStatus } from '../lib/whisper';

type CardState = SenseVoiceModelStatus['state'] | 'loading';

const MODEL_SIZE_LABEL = '约 228 MB';

/**
 * SenseVoice 单模型卡片：状态检查、下载、取消。
 * 进度事件只表示「还在下载」——模型与 tokens 分两段下载，进度会从 100% 回到 0%，
 * 完成一律以 sensevoice-download-complete 事件或重新查询的状态为准。
 */
export function SenseVoiceModelCard({ className = '' }: { className?: string }) {
  const [state, setState] = useState<CardState>('loading');
  const [progress, setProgress] = useState(0);
  const [cancelling, setCancelling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const status = await SenseVoiceAPI.getStatus();
      setState(status.state);
      setProgress(status.progress ?? 0);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setState('missing');
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
      unlistenProgress = await listen<{ progress: number }>('sensevoice-download-progress', (event) => {
        setState('downloading');
        setProgress(event.payload.progress);
      });
      unlistenComplete = await listen('sensevoice-download-complete', () => {
        setCancelling(false);
        void refresh();
      });
      unlistenError = await listen<{ error: string }>('sensevoice-download-error', (event) => {
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
      await SenseVoiceAPI.downloadModel();
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
      await SenseVoiceAPI.cancelDownload();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setCancelling(false);
    }
  };

  return (
    <div className={`rounded-lg border-2 border-gray-200 bg-white p-3 ${className}`}>
      <div className="flex items-start justify-between">
        <div className="flex-1">
          <h3 className="font-semibold text-gray-900">SenseVoice 模型</h3>
          <p className="text-sm text-gray-500 mt-1">
            中文优先的离线识别模型，首次使用需下载（{MODEL_SIZE_LABEL}）
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

          {state === 'missing' && (
            <button
              onClick={() => void startDownload()}
              className="bg-blue-600 text-white px-3 py-1.5 rounded-md text-sm font-medium hover:bg-blue-700 transition-colors"
            >
              {`下载模型（${MODEL_SIZE_LABEL}）`}
            </button>
          )}

          {state === 'corrupted' && (
            <button
              onClick={() => void startDownload()}
              className="bg-orange-600 text-white px-3 py-1.5 rounded-md text-sm font-medium hover:bg-orange-700 transition-colors"
            >
              重新下载
            </button>
          )}
        </div>
      </div>

      {state === 'corrupted' && (
        <p className="text-xs text-orange-600 mt-2">模型文件不完整或已损坏，请重新下载。</p>
      )}

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
