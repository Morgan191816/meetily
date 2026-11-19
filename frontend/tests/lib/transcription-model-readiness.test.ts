import { describe, expect, test } from 'bun:test';

import {
  getProviderCommands,
  hasDownloadingModel,
  mapProviderToCommand,
} from '../../src/lib/transcription-model-readiness';

describe('transcription model readiness', () => {
  test('uses Whisper commands when Whisper is the configured provider', () => {
    expect(getProviderCommands('localWhisper')).toEqual({
      initialize: 'whisper_init',
      hasAvailableModels: 'whisper_has_available_models',
      getAvailableModels: 'whisper_get_available_models',
    });
  });

  test('does not silently treat an unsupported provider as supported', () => {
    expect(getProviderCommands('deepgram')).toBeNull();
    expect(getProviderCommands('parakeet')).toBeNull();
  });

  test('senseVoice 就绪检查走 sensevoice_get_status', () => {
    const calls = mapProviderToCommand('senseVoice');
    expect(calls).toEqual({ statusCommand: 'sensevoice_get_status', downloadCommand: 'sensevoice_download_model' });
  });

  test('recognizes only active downloads', () => {
    expect(hasDownloadingModel([{ status: 'Available' }])).toBeFalse();
    expect(hasDownloadingModel([{ status: 'Downloading' }])).toBeTrue();
    expect(hasDownloadingModel([{ status: { Downloading: { progress: 0 } } }])).toBeTrue();
    expect(hasDownloadingModel([{ status: { Downloading: { progress: 42 } } }])).toBeTrue();
  });
});
