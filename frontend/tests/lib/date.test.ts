import { describe, expect, test } from 'bun:test'
import { formatDate, formatDateTime, formatRelative } from '../../src/lib/date'

describe('date helpers', () => {
  test('formatDate uses zh-CN', () => {
    expect(formatDate('2026-11-17T00:00:00Z')).toMatch(/2026\/11\/17/)
  })

  test('formatDateTime uses zh-CN', () => {
    expect(formatDateTime('2026-11-17T00:00:00Z')).toMatch(/2026\/11\/17/)
  })

  test('formatRelative returns Chinese text', () => {
    const result = formatRelative(new Date(Date.now() - 60_000))
    expect(result).toContain('前')
  })

  test('invalid dates return empty string', () => {
    expect(formatDate('not-a-date')).toBe('')
  })
})
