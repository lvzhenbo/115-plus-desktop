import type { SubtitleItem } from '@/api/types/video';

export type SupportedSubtitleType = 'vtt' | 'srt' | 'ssa' | 'ass';
export type TextSubtitleType = 'vtt' | 'srt';
export type AssSubtitleType = 'ssa' | 'ass';
export type SubtitleRenderMode = 'text' | 'ass';

export interface SubtitleCue {
  start: number;
  end: number;
  text: string;
}

export interface ParsedTextSubtitleTrack {
  mode: 'text';
  item: SubtitleItem;
  type: TextSubtitleType;
  cues: SubtitleCue[];
}

export interface ParsedAssSubtitleTrack {
  mode: 'ass';
  item: SubtitleItem;
  type: AssSubtitleType;
  content: string;
  fontFamilies: string[];
}
