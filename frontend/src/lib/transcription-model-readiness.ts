export interface ModelWithStatus {
  status?: unknown;
}

interface ProviderCommands {
  initialize: string;
  hasAvailableModels: string;
  getAvailableModels: string;
}

const PROVIDER_COMMANDS: Record<string, ProviderCommands> = {
  localWhisper: {
    initialize: 'whisper_init',
    hasAvailableModels: 'whisper_has_available_models',
    getAvailableModels: 'whisper_get_available_models',
  },
};

// 单模型 provider：状态与下载各一条命令，不走 Whisper 的多模型列表。
const PROVIDER_STATUS_COMMANDS: Record<string, { statusCommand: string; downloadCommand: string }> = {
  senseVoice: {
    statusCommand: 'sensevoice_get_status',
    downloadCommand: 'sensevoice_download_model',
  },
};

export function getProviderCommands(provider: string): ProviderCommands | null {
  return PROVIDER_COMMANDS[provider] ?? null;
}

export function mapProviderToCommand(provider: string): { statusCommand: string; downloadCommand: string } | null {
  return PROVIDER_STATUS_COMMANDS[provider] ?? null;
}

export function hasDownloadingModel(models: ModelWithStatus[]): boolean {
  return models.some(({ status }) => (
    status === 'Downloading'
    || (status !== null && typeof status === 'object' && 'Downloading' in status)
  ));
}
