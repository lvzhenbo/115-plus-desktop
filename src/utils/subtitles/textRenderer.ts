import type { SubtitleCue } from './types';

function findStartIndex(cues: SubtitleCue[], currentTime: number): number {
  let lo = 0;
  let hi = cues.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (cues[mid]!.end < currentTime) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  return lo;
}

function collectActiveTexts(cues: SubtitleCue[], currentTime: number): string[] {
  const startIndex = findStartIndex(cues, currentTime);
  const results: string[] = [];

  for (let i = startIndex; i < cues.length; i++) {
    const cue = cues[i]!;
    if (cue.start > currentTime) break;
    if (currentTime >= cue.start && currentTime <= cue.end) {
      const text = cue.text.trim();
      if (text) results.push(text);
    }
  }

  return results;
}

export function collectActiveCueLines(cues: SubtitleCue[], currentTime: number): string[] {
  const seenLines = new Set<string>();

  return collectActiveTexts(cues, currentTime)
    .flatMap((text) => text.split('\n'))
    .map((line) => line.trim())
    .filter((line) => {
      if (!line || seenLines.has(line)) return false;
      seenLines.add(line);
      return true;
    });
}
