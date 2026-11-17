import { formatDistanceToNow } from 'date-fns'
import { zhCN } from 'date-fns/locale'

export function formatDate(input: Date | string): string {
  return new Date(input).toLocaleDateString('zh-CN')
}

export function formatDateTime(input: Date | string): string {
  return new Date(input).toLocaleString('zh-CN')
}

export function formatRelative(input: Date | string): string {
  return formatDistanceToNow(new Date(input), { addSuffix: true, locale: zhCN })
}
