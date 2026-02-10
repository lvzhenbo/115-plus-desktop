<template>
  <div
    v-if="currentLines.length > 0"
    class="absolute bottom-16 left-0 right-0 flex justify-center z-15 pointer-events-none px-8"
  >
    <div class="inline-flex flex-col items-center gap-0.5">
      <span
        v-for="(line, idx) in currentLines"
        :key="idx"
        class="inline-block text-white text-lg px-3 py-0.5 rounded bg-black/60 text-center leading-relaxed"
      >
        {{ line }}
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
  import type { SubtitleItem } from '@/api/types/video';
  import { fetch as tauriFetch } from '@tauri-apps/plugin-http';
  import { parseText } from 'media-captions';

  const props = defineProps<{
    /** 字幕列表 */
    subtitleList: SubtitleItem[];
    /** 当前选中的字幕 sid，没有表示关闭 */
    currentSid?: string;
    /** 是否启用字幕 */
    enabled: boolean;
    /** 当前播放时间（秒） */
    currentTime: number;
  }>();

  const message = useMessage();

  /** 解析后的字幕 cue 列表 */
  const subtitleCues = ref<{ start: number; end: number; text: string }[]>([]);

  /** 当前应显示的字幕文本 */
  const currentSubtitleText = computed(() => {
    if (!props.enabled || subtitleCues.value.length === 0) return '';
    const t = props.currentTime;
    const cue = subtitleCues.value.find((c) => t >= c.start && t <= c.end);
    return cue ? cue.text : '';
  });

  /** 字幕多行数组 */
  const currentLines = computed(() => {
    const text = currentSubtitleText.value;
    if (!text) return [];
    return text.split('\n').filter((l) => l.trim());
  });

  /** 推断字幕文件格式 */
  const detectSubtitleType = (content: string, apiType: string): 'vtt' | 'srt' | 'ssa' | 'ass' => {
    const trimmed = content.trim();
    if (trimmed.startsWith('WEBVTT')) return 'vtt';
    if (/^\[Script Info\]/i.test(trimmed)) {
      return 'ssa';
    }
    const lower = apiType?.toLowerCase() || '';
    if (lower === 'vtt') return 'vtt';
    if (lower === 'ass' || lower === 'ssa') return 'ssa';
    return 'srt';
  };

  /** 加载并解析字幕文件 */
  const loadSubtitle = async () => {
    subtitleCues.value = [];

    if (!props.enabled || !props.currentSid) return;

    const subtitle = props.subtitleList.find((s) => s.sid === props.currentSid);
    if (!subtitle) return;

    try {
      const response = await tauriFetch(subtitle.url);
      const text = await response.text();

      if (!text || text.trim().length === 0) {
        message.warning('字幕文件内容为空');
        return;
      }

      const type = detectSubtitleType(text, subtitle.type);
      const { cues } = await parseText(text, { type });

      subtitleCues.value = cues.map((cue) => ({
        start: cue.startTime,
        end: cue.endTime,
        text: cue.text.replace(/<[^>]*>/g, ''),
      }));
    } catch (e) {
      console.error('字幕加载失败', e);
      message.error('字幕加载失败');
    }
  };

  // 当 sid 或 enabled 变化时自动加载字幕
  watch(
    () => [props.currentSid, props.enabled] as const,
    ([sid, enabled]) => {
      if (!enabled || !sid) {
        subtitleCues.value = [];
        return;
      }
      loadSubtitle();
    },
  );

  /** 暴露给父组件，支持手动触发加载 */
  defineExpose({ loadSubtitle });
</script>
