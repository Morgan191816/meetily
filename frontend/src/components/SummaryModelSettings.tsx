'use client';

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { ModelConfig, ModelSettingsModal } from '@/components/ModelSettingsModal';
import { SummaryLanguageSettings } from '@/components/SummaryLanguageSettings';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Switch } from './ui/switch';
import { useConfig } from '@/contexts/ConfigContext';
import type { MinutesTemplateOption } from '@/hooks/useMinutesGeneration';

interface SummaryModelSettingsProps {
  refetchTrigger?: number; // Change this to trigger refetch
}

export function SummaryModelSettings({ refetchTrigger }: SummaryModelSettingsProps) {
  const [modelConfig, setModelConfig] = useState<ModelConfig>({
    provider: 'ollama',
    model: 'llama3.2:latest',
    whisperModel: 'large-v3',
    apiKey: null,
    ollamaEndpoint: null
  });

  const { isAutoSummary, toggleIsAutoSummary } = useConfig();

  // 默认纪要模板
  const [templates, setTemplates] = useState<MinutesTemplateOption[] | null>(null);
  const [defaultTemplateId, setDefaultTemplateId] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      invoke<MinutesTemplateOption[]>('api_list_minutes_templates'),
      invoke<string>('api_get_default_minutes_template'),
    ])
      .then(([list, current]) => {
        setTemplates(list);
        setDefaultTemplateId(current);
      })
      .catch((error) => {
        console.error('加载纪要模板失败:', error);
        toast.error('加载纪要模板失败');
      });
  }, []);

  const handleTemplateChange = async (templateId: string) => {
    const previous = defaultTemplateId;
    setDefaultTemplateId(templateId);
    try {
      await invoke('api_set_default_minutes_template', { templateId });
      toast.success('默认模板已保存');
    } catch (error) {
      console.error('保存默认纪要模板失败:', error);
      setDefaultTemplateId(previous);
      toast.error('保存默认模板失败');
    }
  };

  // Reusable fetch function
  const fetchModelConfig = useCallback(async () => {
    try {
      const data = await invoke('api_get_model_config') as any;
      if (data && data.provider !== null) {
        // Fetch API key if not included and provider requires it
        if (data.provider !== 'ollama' && !data.apiKey) {
          try {
            const apiKeyData = await invoke('api_get_api_key', {
              provider: data.provider
            }) as string;
            data.apiKey = apiKeyData;
          } catch (err) {
            console.error('Failed to fetch API key:', err);
          }
        }
        // Fetch Custom OpenAI config if that's the active provider
        if (data.provider === 'custom-openai') {
          try {
            const customConfig = (await invoke('api_get_custom_openai_config')) as any;
            if (customConfig) {
              data.customOpenAIDisplayName = customConfig.displayName || null;
              data.customOpenAIEndpoint = customConfig.endpoint || null;
              data.customOpenAIModel = customConfig.model || null;
              data.customOpenAIApiKey = customConfig.apiKey || null;
              data.maxTokens = customConfig.maxTokens || null;
              data.temperature = customConfig.temperature || null;
              data.topP = customConfig.topP || null;
              // For custom-openai, model field should match customOpenAIModel
              data.model = customConfig.model || data.model;
            }
          } catch (err) {
            console.error('Failed to fetch custom OpenAI config:', err);
          }
        }
        setModelConfig(data);
      }
    } catch (error) {
      console.error('Failed to fetch model config:', error);
      toast.error('加载模型设置失败');
    }
  }, []);

  // Fetch on mount
  useEffect(() => {
    fetchModelConfig();
  }, [fetchModelConfig]);

  // Refetch when trigger changes (optional external control)
  useEffect(() => {
    if (refetchTrigger !== undefined && refetchTrigger > 0) {
      fetchModelConfig();
    }
  }, [refetchTrigger, fetchModelConfig]);

  // Listen for model config updates from other components
  useEffect(() => {
    const setupListener = async () => {
      const { listen } = await import('@tauri-apps/api/event');
      const unlisten = await listen<ModelConfig>('model-config-updated', (event) => {
        console.log('SummaryModelSettings received model-config-updated event:', event.payload);
        setModelConfig(event.payload);
      });

      return unlisten;
    };

    let cleanup: (() => void) | undefined;
    setupListener().then(fn => cleanup = fn);

    return () => {
      cleanup?.();
    };
  }, []);

  // Save handler
  const handleSaveModelConfig = async (config: ModelConfig) => {
    try {
      await invoke('api_save_model_config', {
        provider: config.provider,
        model: config.model,
        whisperModel: config.whisperModel,
        apiKey: config.apiKey,
        ollamaEndpoint: config.ollamaEndpoint,
      });

      setModelConfig(config);

      // Emit event to sync other components
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', config);

      toast.success('模型设置已保存');
    } catch (error) {
      console.error('Error saving model config:', error);
      toast.error('保存模型设置失败');
    }
  };

  return (
    <div className='flex flex-col gap-4'>
      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-lg font-semibold text-gray-900 mb-2">自动生成纪要</h3>
            <p className="text-sm text-gray-600">会议结束（停止录音）后自动生成纪要</p>
          </div>
          <Switch checked={isAutoSummary} onCheckedChange={toggleIsAutoSummary} />
        </div>
      </div>

      <SummaryLanguageSettings />

      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <h3 className="text-lg font-semibold mb-4">纪要模型配置</h3>
        <p className="text-sm text-gray-600 mb-6">
          配置用于生成会议纪要的 AI 模型。
        </p>

        {modelConfig.provider !== 'custom-openai' && (
          <p className="mb-6 text-sm text-amber-700">
            会议纪要仅使用「自定义服务（OpenAI / Anthropic 兼容）」的配置生成；当前选择的其它服务不会用于纪要。
          </p>
        )}

        <ModelSettingsModal
          modelConfig={modelConfig}
          setModelConfig={setModelConfig}
          onSave={handleSaveModelConfig}
          skipInitialFetch={true}
        />
      </div>

      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <h3 className="text-lg font-semibold mb-2">默认纪要模板</h3>
        <p className="text-sm text-gray-600 mb-4">
          生成纪要时默认使用的模板；单次会议仍可在纪要页临时切换。
        </p>

        {templates && defaultTemplateId ? (
          <Select value={defaultTemplateId} onValueChange={handleTemplateChange}>
            <SelectTrigger className="w-64" aria-label="默认纪要模板">
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
        ) : (
          <p className="text-sm text-gray-500">加载中…</p>
        )}
      </div>
    </div>
  );
}
