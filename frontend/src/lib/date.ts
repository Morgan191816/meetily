import { formatDistanceToNow } from 'date-fns'
import { zhCN } from 'date-fns/locale'

function toValidDate(input: Date | string | number): Date | null {
  const d = new Date(input)
  return Number.isNaN(d.getTime()) ? null : d
}

export function formatDate(input: Date | string | number): string {
  const d = toValidDate(input)
  return d ? d.toLocaleDateString('zh-CN') : ''
}

export function formatDateTime(input: Date | string | number): string {
  const d = toValidDate(input)
  return d ? d.toLocaleString('zh-CN') : ''
}

export function formatRelative(input: Date | string | number): string {
  const d = toValidDate(input)
  return d ? formatDistanceToNow(d, { addSuffix: true, locale: zhCN }) : ''
}
