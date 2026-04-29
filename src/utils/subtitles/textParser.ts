import type { SubtitleItem } from '@/api/types/video';
import { detectSubtitleType } from './detect';
import type { ParsedTextSubtitleTrack, SubtitleCue, TextSubtitleType } from './types';

const TIMESTAMP_RE = /(\d{1,}:)?(\d{1,}):(\d{1,})[.,](\d{1,3})/;

function parseTimestamp(raw: string): number {
  const m = raw.trim().match(TIMESTAMP_RE);
  if (!m) return 0;
  const h = parseInt(m[1] || '0', 10);
  const min = parseInt(m[2], 10);
  const sec = parseInt(m[3], 10);
  const ms = parseInt(m[4].padEnd(3, '0'), 10);
  return h * 3600 + min * 60 + sec + ms / 1000;
}

function stripHtmlTags(text: string): string {
  return text.replace(/<[^>]*>/g, '');
}

function stripBom(content: string): string {
  return content.replace(/^\uFEFF/, '');
}

const VTT_META_PREFIXES = /^(WEBVTT|NOTE|STYLE|REGION|KIND|LANGUAGE|CHARACTER)/i;

function parseCues(content: string): SubtitleCue[] {
  const normalized = stripBom(content).replace(/\r\n/g, '\n');
  const blocks = normalized.split(/\n\n/);
  const cues: SubtitleCue[] = [];

  for (const block of blocks) {
    const trimmed = block.trim();
    if (!trimmed) continue;
    if (VTT_META_PREFIXES.test(trimmed)) continue;

    const lines = block.split('\n');
    let tsLineIdx = -1;

    for (let i = 0; i < lines.length; i++) {
      if (lines[i]!.includes('-->')) {
        tsLineIdx = i;
        break;
      }
    }

    if (tsLineIdx === -1) continue;

    const tsLine = lines[tsLineIdx]!;
    const parts = tsLine.split(/\s*-->\s*/);
    if (parts.length < 2) continue;

    const start = parseTimestamp(parts[0]!);
    const end = parseTimestamp(parts[1]!.split(/\s+/)[0]!);

    const textLines = lines
      .slice(tsLineIdx + 1)
      .join('\n')
      .trim();
    if (!textLines) continue;

    cues.push({ start, end, text: stripHtmlTags(textLines) });
  }

  return cues;
}

export async function parseTextSubtitle(
  subtitle: SubtitleItem,
  content: string,
  type?: TextSubtitleType,
): Promise<ParsedTextSubtitleTrack> {
  const resolvedType = type ?? detectSubtitleType(content, subtitle.type);
  if (resolvedType === 'ass' || resolvedType === 'ssa') {
    throw new Error('ASS/SSA 字幕不能走文本字幕解析器');
  }

  return {
    mode: 'text',
    item: subtitle,
    type: resolvedType,
    cues: parseCues(content),
  };
}
