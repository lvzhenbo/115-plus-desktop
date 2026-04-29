import type { SupportedSubtitleType } from './types';

export function detectSubtitleType(content: string, apiType?: string): SupportedSubtitleType {
  const trimmed = content.trim();
  const lower = apiType?.toLowerCase() || '';

  if (trimmed.startsWith('WEBVTT')) return 'vtt';
  if (/^\[V4\+ Styles\]/im.test(content)) return 'ass';
  if (/^\[Script Info\]/i.test(trimmed)) {
    return lower === 'ssa' ? 'ssa' : 'ass';
  }

  if (lower === 'vtt') return 'vtt';
  if (lower === 'ass' || lower === 'ssa') return lower;
  return 'srt';
}
