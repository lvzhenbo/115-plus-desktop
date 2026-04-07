<template>
  <div class="flex items-center mb-2">
    <NTooltip :show="showTooltip" :x="tooltipX" :y="height - 70" placement="top">
      {{ formatTime(hoverTime) }}
    </NTooltip>
    <div
      ref="progressBarRef"
      class="flex-1 h-1.5 bg-white/20 rounded cursor-pointer relative group"
      @mousedown="handleProgressMouseDown"
      @mousemove="handleProgressHover"
      @mouseenter="showTooltip = true"
      @mouseleave="showTooltip = false"
    >
      <!-- 播放进度条 -->
      <NEl
        class="h-full bg-(--primary-color) rounded absolute top-0 left-0"
        :style="{ width: `${progress}%` }"
      ></NEl>
      <!-- 进度条拖动手柄 -->
      <NEl
        class="h-3 w-3 rounded-full bg-(--primary-color) absolute top-1/2 -translate-y-1/2 -ml-1.5 opacity-0 group-hover:opacity-100 transition-opacity shadow-md"
        :class="{ 'opacity-100!': isDraggingProgress }"
        :style="{ left: `${progress}%` }"
      ></NEl>
    </div>
    <div class="ml-4 text-white text-sm min-w-25 text-right tabular-nums">
      {{ formatTime(currentTime) }} / {{ formatTime(duration) }}
    </div>
  </div>
  <!-- 控制栏 -->
  <div class="flex items-center">
    <!-- 控制栏左侧 -->
    <div class="flex items-center gap-1">
      <NButton quaternary circle :disabled="!hasPreviousVideo" @click="emit('previousVideo')">
        <template #icon>
          <NIcon size="24" class="text-white"><StepBackwardOutlined /></NIcon>
        </template>
      </NButton>
      <NButton quaternary circle @click="handleTogglePlay">
        <template #icon>
          <NIcon size="24" class="text-white">
            <PauseCircleOutlined v-if="playing" />
            <PlayCircleOutlined v-else />
          </NIcon>
        </template>
      </NButton>
      <NButton quaternary circle :disabled="!hasNextVideo" @click="emit('nextVideo')">
        <template #icon>
          <NIcon size="24" class="text-white"><StepForwardOutlined /></NIcon>
        </template>
      </NButton>
      <NButton quaternary circle @click="toggleMute">
        <template #icon>
          <NIcon size="24" class="text-white">
            <VolumeMuteFilled v-if="muted" />
            <VolumeUpFilled v-else-if="volumeLevel > 50" />
            <VolumeDownFilled v-else />
          </NIcon>
        </template>
      </NButton>
      <NSlider
        v-model:value="volumeLevel"
        class="w-30! ml-2"
        :min="0"
        :max="100"
        @update:value="changeVolume"
      />
    </div>
    <!-- 控制栏右侧 -->
    <div class="flex items-center ml-auto gap-1">
      <!-- 分辨率选择 -->
      <NPopselect
        :value="currentResolution"
        :options="resolutions"
        @update:value="handleResolutionChange"
      >
        <NButton quaternary round size="small" class="text-white!">
          {{ currentResolutionLabel }}
        </NButton>
      </NPopselect>
      <!-- 播放速度选择 -->
      <NPopselect :value="rate" :options="playbackSpeeds" @update:value="handlePlaybackSpeedChange">
        <NButton quaternary round size="small" class="text-white!"> {{ rate }}x </NButton>
      </NPopselect>
      <!-- 字幕选择 -->
      <NPopselect
        v-if="subtitleList.length > 0"
        v-model:value="currentSubtitleValue"
        :options="subtitleOptions"
      >
        <NButton quaternary round size="small" class="text-white!">
          {{ currentSubtitleLabel }}
        </NButton>
      </NPopselect>
      <!-- 视频旋转 -->
      <NButton quaternary circle @click="emit('rotateVideo')">
        <template #icon>
          <NIcon size="24" class="text-white"><RotateRightOutlined /></NIcon>
        </template>
      </NButton>
      <!-- 播放列表 -->
      <NButton
        quaternary
        circle
        :disabled="!videoListLength"
        @click="videoListShow = !videoListShow"
      >
        <template #icon>
          <NIcon size="24" class="text-white"><UnorderedListOutlined /></NIcon>
        </template>
      </NButton>
      <!-- 全屏切换 -->
      <NButton quaternary circle class="hidden md:flex" @click="emit('toggleFullscreen')">
        <template #icon>
          <NIcon size="24" class="text-white">
            <FullscreenExitOutlined v-if="isFullscreen" />
            <FullscreenOutlined v-else />
          </NIcon>
        </template>
      </NButton>
    </div>
  </div>
</template>

<script setup lang="ts">
  import {
    PlayCircleOutlined,
    PauseCircleOutlined,
    StepBackwardOutlined,
    StepForwardOutlined,
    FullscreenOutlined,
    FullscreenExitOutlined,
    UnorderedListOutlined,
    RotateRightOutlined,
  } from '@vicons/antd';
  import { VolumeDownFilled, VolumeMuteFilled, VolumeUpFilled } from '@vicons/material';
  import type { SubtitleItem } from '@/api/types/video';
  import { type SelectOption } from 'naive-ui';

  const playing = defineModel<boolean>('playing', { required: true });
  const muted = defineModel<boolean>('muted', { required: true });
  const volumeLevel = defineModel<number>('volumeLevel', { required: true });
  const rate = defineModel<number>('rate', { required: true });
  const currentResolution = defineModel<number>('currentResolution', { required: true });
  const currentSubtitleValue = defineModel<string>('currentSubtitleValue', { required: true });
  const videoListShow = defineModel<boolean>('videoListShow', { required: true });

  const props = defineProps<{
    currentTime: number;
    duration: number;
    progress: number;
    isFullscreen: boolean;
    hasPreviousVideo: boolean;
    hasNextVideo: boolean;
    resolutions: SelectOption[];
    subtitleList: SubtitleItem[];
    videoListLength: number;
  }>();

  const emit = defineEmits<{
    previousVideo: [];
    nextVideo: [];
    toggleFullscreen: [];
    changeResolution: [value: number];
    changePlaybackSpeed: [speed: number];
    rotateVideo: [];
    seek: [time: number];
    togglePlay: [];
    toggleMute: [];
    changeVolume: [value: number];
  }>();

  const { height } = useWindowSize();
  const progressBarRef = ref<HTMLElement | null>(null);
  const isDraggingProgress = ref(false);
  const showTooltip = ref(false);
  const tooltipX = ref(0);
  const hoverTime = ref(0);

  const playbackSpeeds: SelectOption[] = [
    { label: '5x', value: 5 },
    { label: '4x', value: 4 },
    { label: '3x', value: 3 },
    { label: '2x', value: 2 },
    { label: '1.5x', value: 1.5 },
    { label: '1.25x', value: 1.25 },
    { label: '1x', value: 1 },
    { label: '0.75x', value: 0.75 },
    { label: '0.5x', value: 0.5 },
  ];

  // 分辨率标签
  const currentResolutionLabel = computed(() => {
    const resolution = props.resolutions.find((res) => res.value === currentResolution.value);
    return resolution ? resolution.label : '';
  });

  // 字幕选项
  const subtitleOptions = computed(() => {
    const options: SelectOption[] = [{ label: '关闭字幕', value: '__off__' }];
    props.subtitleList.forEach((s) => {
      options.push({
        label: s.title || s.language || s.file_name || '未知字幕',
        value: s.sid,
      });
    });
    return options;
  });

  // 字幕标签
  const currentSubtitleLabel = computed(() => {
    const val = currentSubtitleValue.value;
    if (val === '__off__') return '字幕';
    const subtitle = props.subtitleList.find((s) => s.sid === val);
    return subtitle ? subtitle.title || subtitle.language || '字幕' : '字幕';
  });

  // 格式化时间显示
  const formatTime = (time: number): string => {
    if (!Number.isFinite(time) || time < 0) return '00:00:00';
    const hours = Math.floor(time / 3600);
    const minutes = Math.floor((time % 3600) / 60);
    const seconds = Math.floor(time % 60);
    return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
  };

  // 进度条点击与拖拽
  const handleProgressMouseDown = (e: MouseEvent) => {
    if (!progressBarRef.value) return;

    isDraggingProgress.value = true;
    const wasPlaying = playing.value;
    if (wasPlaying) playing.value = false;

    const seekToPosition = (clientX: number) => {
      const rect = progressBarRef.value!.getBoundingClientRect();
      const position = Math.max(0, Math.min((clientX - rect.left) / rect.width, 1));
      emit('seek', position * props.duration);
    };

    // 立即跳转到点击位置
    seekToPosition(e.clientX);

    const onMouseMove = (moveEvent: MouseEvent) => {
      seekToPosition(moveEvent.clientX);
    };

    const onMouseUp = () => {
      isDraggingProgress.value = false;
      if (wasPlaying) playing.value = true;
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
    };

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
  };

  const handleProgressHover = (e: MouseEvent) => {
    if (!progressBarRef.value || !props.duration) return;
    const rect = progressBarRef.value.getBoundingClientRect();
    const position = Math.max(0, Math.min((e.clientX - rect.left) / rect.width, 1));
    hoverTime.value = position * props.duration;
    tooltipX.value = e.clientX;
  };

  // 音量
  const changeVolume = (val: number) => {
    volumeLevel.value = val;
    muted.value = val === 0;
    emit('changeVolume', val);
  };

  const toggleMute = () => {
    muted.value = !muted.value;
    emit('toggleMute');
  };

  // 播放/暂停
  const handleTogglePlay = () => {
    playing.value = !playing.value;
    emit('togglePlay');
  };

  // 分辨率切换
  const handleResolutionChange = (value: number) => {
    currentResolution.value = value;
    emit('changeResolution', value);
  };

  // 播放速度切换
  const handlePlaybackSpeedChange = (speed: number) => {
    rate.value = speed;
    emit('changePlaybackSpeed', speed);
  };
</script>
