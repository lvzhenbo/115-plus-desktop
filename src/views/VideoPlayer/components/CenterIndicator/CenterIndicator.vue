<template>
  <AnimatePresence>
    <div
      v-if="visible"
      :key="indicatorKey"
      v-motion
      :initial="{ opacity: 0, scale: 0.6 }"
      :animate="{ opacity: 1, scale: 1 }"
      :exit="{ opacity: 0, scale: 1.2 }"
      :transition="{ duration: 0.2 }"
      class="absolute inset-0 flex justify-center items-center z-15 pointer-events-none"
    >
      <div
        class="bg-black/50 rounded-full w-20 h-20 flex flex-col justify-center items-center gap-0.5"
      >
        <NIcon v-if="indicatorIcon" :size="36" class="text-white">
          <component :is="indicatorIcon" />
        </NIcon>
        <span
          v-if="indicatorText"
          class="text-white font-medium"
          :class="indicatorIcon ? 'text-xs' : 'text-lg'"
        >
          {{ indicatorText }}
        </span>
      </div>
    </div>
  </AnimatePresence>
</template>

<script setup lang="ts">
  import { FastForwardOutlined, FastBackwardOutlined } from '@vicons/antd';
  import {
    VolumeUpFilled,
    VolumeDownFilled,
    VolumeMuteFilled,
    PlayArrowFilled,
    PauseFilled,
  } from '@vicons/material';
  import { vMotion } from 'motion-v';
  import type { Component } from 'vue';

  type IndicatorType = 'play' | 'pause' | 'forward' | 'backward' | 'volume' | 'mute' | 'speed';

  const visible = ref(false);
  const indicatorKey = ref(0);
  const indicatorType = ref<IndicatorType>('play');
  const indicatorValue = ref<string | number>('');

  let hideTimer: ReturnType<typeof setTimeout> | null = null;

  const indicatorIcon = computed<Component | null>(() => {
    switch (indicatorType.value) {
      case 'play':
        return PlayArrowFilled;
      case 'pause':
        return PauseFilled;
      case 'forward':
        return FastForwardOutlined;
      case 'backward':
        return FastBackwardOutlined;
      case 'volume':
        return Number(indicatorValue.value) > 50 ? VolumeUpFilled : VolumeDownFilled;
      case 'mute':
        return VolumeMuteFilled;
      case 'speed':
        return null;
      default:
        return null;
    }
  });

  const indicatorText = computed(() => {
    switch (indicatorType.value) {
      case 'forward':
      case 'backward':
        return indicatorValue.value ? `${indicatorValue.value}秒` : '';
      case 'volume':
        return `${indicatorValue.value}%`;
      case 'speed':
        return `${indicatorValue.value}x`;
      default:
        return '';
    }
  });

  const show = (type: IndicatorType, value?: string | number) => {
    indicatorType.value = type;
    indicatorValue.value = value ?? '';
    indicatorKey.value++;
    visible.value = true;
    if (hideTimer) clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      visible.value = false;
    }, 600);
  };

  defineExpose({ show });
</script>
