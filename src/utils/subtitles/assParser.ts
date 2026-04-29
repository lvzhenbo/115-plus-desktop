import type { SubtitleItem } from '@/api/types/video';
import type { AssSubtitleType, ParsedAssSubtitleTrack } from './types';
import { normalizeFontFamily } from './font';

function collectFontFamily(fontFamilies: string[], seen: Set<string>, fontFamily: string) {
  const normalized = normalizeFontFamily(fontFamily);
  if (!normalized) return;

  const key = normalized.toLowerCase();
  if (seen.has(key)) return;

  seen.add(key);
  fontFamilies.push(normalized);
}

function extractStyleFontFamilies(content: string) {
  const fontFamilies: string[] = [];
  const seen = new Set<string>();
  const lines = content.split(/\r?\n/);
  let inStyleSection = false;
  let fontNameIndex = -1;

  for (const line of lines) {
    const sectionMatch = line.match(/^\s*\[(.+?)\]\s*$/);
    if (sectionMatch) {
      inStyleSection = /^(V4\+ Styles|V4 Styles)$/i.test(sectionMatch[1] ?? '');
      fontNameIndex = -1;
      continue;
    }

    if (!inStyleSection) continue;

    if (/^\s*Format:/i.test(line)) {
      const fields = line
        .slice(line.indexOf(':') + 1)
        .split(',')
        .map((field) => field.trim().toLowerCase());
      fontNameIndex = fields.findIndex((field) => field === 'fontname');
      continue;
    }

    if (fontNameIndex < 0 || !/^\s*Style:/i.test(line)) continue;

    const values = line.slice(line.indexOf(':') + 1).split(',');
    const fontFamily = values[fontNameIndex];
    if (!fontFamily) continue;

    collectFontFamily(fontFamilies, seen, fontFamily);
  }

  return { fontFamilies, seen };
}

function extractOverrideFontFamilies(content: string, fontFamilies: string[], seen: Set<string>) {
  const overrideFontPattern = /\\fn([^\\}\r\n]+)/g;

  for (const match of content.matchAll(overrideFontPattern)) {
    const fontFamily = match[1];
    if (!fontFamily) continue;

    collectFontFamily(fontFamilies, seen, fontFamily);
  }
}

function extractAssFontFamilies(content: string) {
  const { fontFamilies, seen } = extractStyleFontFamilies(content);
  extractOverrideFontFamilies(content, fontFamilies, seen);
  return fontFamilies;
}

export function parseAssSubtitle(
  subtitle: SubtitleItem,
  content: string,
  type: AssSubtitleType,
): ParsedAssSubtitleTrack {
  return {
    mode: 'ass',
    item: subtitle,
    type,
    content,
    fontFamilies: extractAssFontFamilies(content),
  };
}
