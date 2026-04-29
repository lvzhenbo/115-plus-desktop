import type { SubtitleItem } from '@/api/types/video';
import { computed, ref, type Ref } from 'vue';
import { detectSubtitleType } from '@/utils/subtitles/detect';
import type {
  ParsedAssSubtitleTrack,
  ParsedTextSubtitleTrack,
  SubtitleRenderMode,
} from '@/utils/subtitles/types';
import { parseAssSubtitle } from '@/utils/subtitles/assParser';
import { parseTextSubtitle } from '@/utils/subtitles/textParser';
import { collectActiveCueLines } from '@/utils/subtitles/textRenderer';
import { loadSubtitleContent } from '@/utils/subtitles/loader';

interface SubtitleControllerOptions {
  currentTime: Ref<number>;
  enabled: Ref<boolean>;
  onAssTrack?: (track: ParsedAssSubtitleTrack) => Promise<void> | void;
  onClear?: () => Promise<void> | void;
  onEmpty?: () => void;
  onError?: (error: unknown) => void;
}

export function useSubtitleController(options: SubtitleControllerOptions) {
  const parsedTrack = ref<ParsedTextSubtitleTrack | null>(null);
  const renderMode = ref<SubtitleRenderMode | null>(null);
  let requestId = 0;
  let abortController: AbortController | null = null;

  tryOnScopeDispose(() => {
    requestId += 1;
    abortController?.abort();
    abortController = null;
  });

  const currentLines = computed(() => {
    if (!options.enabled.value || !parsedTrack.value || renderMode.value !== 'text') return [];

    return collectActiveCueLines(parsedTrack.value.cues, options.currentTime.value);
  });

  const clear = async () => {
    requestId += 1;
    abortController?.abort();
    abortController = null;
    parsedTrack.value = null;
    renderMode.value = null;
    await options.onClear?.();
  };

  const loadTrack = async (subtitle?: SubtitleItem | null) => {
    const currentRequestId = ++requestId;
    abortController?.abort();
    abortController = new AbortController();
    const signal = abortController.signal;

    // 先清理当前渲染状态，确保切换字幕时不会有闪烁
    parsedTrack.value = null;
    renderMode.value = null;
    await options.onClear?.();

    if (!subtitle) return;

    try {
      const text = await loadSubtitleContent(subtitle, signal);
      if (currentRequestId !== requestId) return;

      if (!text || text.trim().length === 0) {
        options.onEmpty?.();
        return;
      }

      const type = detectSubtitleType(text, subtitle.type);

      if (type === 'ass' || type === 'ssa') {
        renderMode.value = 'ass';
        await options.onAssTrack?.(parseAssSubtitle(subtitle, text, type));
        return;
      }

      const track = await parseTextSubtitle(subtitle, text, type);
      if (currentRequestId !== requestId) return;

      parsedTrack.value = track;
      renderMode.value = 'text';
    } catch (error) {
      if (currentRequestId !== requestId) return;
      if (error instanceof DOMException && error.name === 'AbortError') return;
      renderMode.value = null;
      options.onError?.(error);
    }
  };

  return {
    currentLines,
    renderMode,
    clear,
    loadTrack,
  };
}
