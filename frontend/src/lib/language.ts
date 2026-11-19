export function languageDisplayName(code: string): string {
  if (!code || code === 'auto') return '自动检测'
  try {
    return new Intl.DisplayNames(['zh-CN'], { type: 'language' }).of(code) ?? code
  } catch {
    return code
  }
}
