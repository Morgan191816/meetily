import { expect, test } from 'bun:test';
import { languageDisplayName } from '../../src/lib/language';

test('shows language names in Chinese while keeping codes', () => {
  expect(languageDisplayName('en')).toContain('英');
  expect(languageDisplayName('zh')).toContain('中');
  expect(languageDisplayName('auto')).toBe('自动检测');
});
