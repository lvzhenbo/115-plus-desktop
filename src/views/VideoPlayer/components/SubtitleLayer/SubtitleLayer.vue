<template>
  <canvas
    ref="assCanvasRef"
    class="absolute inset-0 z-15 pointer-events-none"
    style="visibility: hidden"
  ></canvas>
  <div
    v-if="renderMode !== 'ass' && currentLines.length > 0"
    class="absolute left-0 right-0 flex justify-center z-15 pointer-events-none px-8"
    :style="{ bottom: `${subtitleStyle.bottomOffset}px` }"
  >
    <div class="inline-flex flex-col items-center gap-0.5">
      <span
        v-for="(line, idx) in currentLines"
        :key="idx"
        class="inline-block px-3 py-0.5 rounded text-center leading-relaxed"
        :style="subtitleTextStyle"
      >
        {{ line }}
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
  import type { SubtitleItem } from '@/api/types/video';
  import { useSettingStore } from '@/store/setting';
  import { generateTextShadow } from '@/utils/subtitleStyleUtils';
  import { useSubtitleController } from '@/composables/useSubtitleController';
  import { AssSubtitleRenderer } from '@/utils/subtitles/assRenderer';
  import type { CSSProperties } from 'vue';

  const settingStore = useSettingStore();

  const props = defineProps<{
    /** 字幕列表 */
    subtitleList: SubtitleItem[];
    /** 当前选中的字幕 sid，没有表示关闭 */
    currentSid?: string;
    /** 是否启用字幕 */
    enabled: boolean;
    /** 当前播放时间（秒） */
    currentTime: number;
    /** 当前视频元素，用于 ASS 渲染器 */
    videoElement?: HTMLVideoElement | null;
  }>();

  const message = useMessage();

  /** 字幕样式配置（响应式） */
  const subtitleStyle = computed(() => settingStore.subtitleStyleSetting);

  const assCanvasRef = useTemplateRef('assCanvasRef');
  const isMounted = ref(true);

  /** 字幕文字动态样式 */
  const subtitleTextStyle = computed<CSSProperties>(() => {
    const s = subtitleStyle.value;
    return {
      color: s.fontColor,
      fontSize: `${s.fontSize}px`,
      fontWeight: s.fontBold ? 'bold' : 'normal',
      backgroundColor: s.backgroundColor,
      textShadow: generateTextShadow(s.strokeColor, s.strokeWidth),
    };
  });

  const enabledRef = toRef(props, 'enabled');
  const currentTimeRef = toRef(props, 'currentTime');
  const assRenderer = new AssSubtitleRenderer(
    () => props.videoElement ?? null,
    () => assCanvasRef.value,
    (families) => {
      if (families.length > 0) {
        console.warn('系统未找到字体:', families.join('、'));
      }
    },
  );

  const subtitleController = useSubtitleController({
    currentTime: currentTimeRef,
    enabled: enabledRef,
    onAssTrack: async (track) => {
      await assRenderer.loadTrack(track);
    },
    onClear: async () => {
      await assRenderer.clearTrack();
    },
    onEmpty: () => {
      if (isMounted.value) {
        message.warning('字幕文件内容为空');
      }
    },
    onError: (error) => {
      if (isMounted.value) {
        console.error('字幕加载失败', error);
        message.error('字幕加载失败');
      }
    },
  });

  const currentLines = subtitleController.currentLines;
  const renderMode = subtitleController.renderMode;

  onBeforeUnmount(() => {
    isMounted.value = false;
    void assRenderer.destroy().catch(() => {});
  });

  /** 加载并解析字幕文件 */
  const loadSubtitle = async () => {
    if (!props.enabled || !props.currentSid) {
      await subtitleController.clear();
      return;
    }

    const subtitle = props.subtitleList.find((s) => s.sid === props.currentSid);
    await subtitleController.loadTrack(subtitle ?? null);
  };

  const debouncedLoadSubtitle = useDebounceFn(() => {
    void loadSubtitle();
  }, 300);

  // 当 sid 或 enabled 变化时自动加载字幕
  watch(
    () => [props.currentSid, props.enabled] as const,
    ([sid, enabled]) => {
      if (!enabled || !sid) {
        void subtitleController.clear();
        return;
      }
      debouncedLoadSubtitle();
    },
  );

  /** 暴露给父组件，支持手动触发加载 */
  defineExpose({ loadSubtitle });
</script>
