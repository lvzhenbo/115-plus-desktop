<template>
  <div class="w-full h-full">
    <div class="bg-[#1a1a1a] h-screen w-screen overflow-hidden">
      <div
        ref="videoContainer"
        class="relative w-full h-full bg-black overflow-hidden flex justify-center items-center"
        @mousemove="handleMouseMove"
        @mouseleave="hideControlsDelayed"
      >
        <video
          ref="videoRef"
          class="w-full h-full object-contain cursor-pointer"
          :class="{ 'cursor-none!': !controlsVisible && playing }"
          @click="handleClick"
          @dblclick="handleDblClick"
        ></video>
        <!-- 视频加载中 -->
        <AnimatePresence>
          <motion.div
            v-if="waiting || seeking"
            key="loading"
            :animate="{ opacity: 1 }"
            :initial="{ opacity: 0 }"
            :exit="{ opacity: 0 }"
            :transition="{ duration: 0.3 }"
            class="absolute inset-0 flex flex-col justify-center items-center gap-4 bg-black/50 text-white z-10 pointer-events-none"
          >
            <NSpin size="large" />
            <span class="text-sm">加载中...</span>
          </motion.div>
        </AnimatePresence>
        <!-- 视频标题 -->
        <AnimatePresence>
          <motion.div
            v-if="controlsVisible && file"
            key="title"
            :animate="{ opacity: 1, y: 0 }"
            :initial="{ opacity: 0, y: -20 }"
            :exit="{ opacity: 0, y: -20 }"
            class="absolute top-0 left-0 w-full px-4 py-3 bg-linear-to-b from-black/80 to-transparent z-20 pointer-events-none"
          >
            <NEllipsis class="text-white text-sm font-medium">
              {{ file.fn }}
            </NEllipsis>
          </motion.div>
        </AnimatePresence>
        <!-- 字幕显示层 -->
        <SubtitleLayer
          ref="subtitleLayerRef"
          :subtitle-list="subtitleList"
          :current-sid="currentSubtitleSid"
          :enabled="subtitleEnabled"
          :current-time="currentTime"
        />
        <!-- 视频控制条 -->
        <AnimatePresence>
          <motion.div
            v-if="controlsVisible"
            key="controls"
            ref="controlsRef"
            :animate="{ opacity: 1, y: 0 }"
            :initial="{ opacity: 0, y: 20 }"
            :exit="{ opacity: 0, y: 20 }"
            class="absolute bottom-0 left-0 w-full px-4 py-2 bg-linear-to-t from-black/90 to-transparent z-20 box-border"
          >
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
                <!-- 缓冲进度条 -->
                <div
                  class="h-full bg-white/30 rounded absolute top-0 left-0 transition-[width] duration-200"
                  :style="{ width: `${bufferedProgress}%` }"
                ></div>
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
                <NButton
                  quaternary
                  circle
                  :disabled="!hasPreviousVideo"
                  @click="handlePreviousVideo"
                >
                  <template #icon>
                    <NIcon size="24" class="text-white"><StepBackwardOutlined /></NIcon>
                  </template>
                </NButton>
                <NButton quaternary circle @click="playing = !playing">
                  <template #icon>
                    <NIcon size="24" class="text-white">
                      <PauseCircleOutlined v-if="playing" />
                      <PlayCircleOutlined v-else />
                    </NIcon>
                  </template>
                </NButton>
                <NButton quaternary circle :disabled="!hasNextVideo" @click="handleNextVideo">
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
                  v-model:value="currentResolution"
                  :options="resolutions"
                  @update:value="changeResolution"
                >
                  <NButton quaternary round size="small" class="text-white!">
                    {{ currentResolutionLabel }}
                  </NButton>
                </NPopselect>
                <!-- 播放速度选择 -->
                <NPopselect
                  v-model:value="rate"
                  :options="playbackSpeeds"
                  @update:value="changePlaybackSpeed"
                >
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
                <!-- 播放列表 -->
                <NButton
                  quaternary
                  circle
                  :disabled="!videoList.length"
                  @click="videoListShow = !videoListShow"
                >
                  <template #icon>
                    <NIcon size="24" class="text-white"><UnorderedListOutlined /></NIcon>
                  </template>
                </NButton>
                <!-- 全屏切换 -->
                <NButton quaternary circle class="hidden md:flex" @click="toggleFullscreen">
                  <template #icon>
                    <NIcon size="24" class="text-white">
                      <FullscreenExitOutlined v-if="isFullscreen" />
                      <FullscreenOutlined v-else />
                    </NIcon>
                  </template>
                </NButton>
              </div>
            </div>
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
    <VideoListDrawer
      v-model:show="videoListShow"
      v-model:pick-code="pickCode"
      :video-list
      @update:pick-code="handleChangeVideo"
    />
  </div>
</template>

<script setup lang="ts">
  import Hls from 'hls.js';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import {
    PlayCircleOutlined,
    PauseCircleOutlined,
    StepBackwardOutlined,
    StepForwardOutlined,
    FullscreenOutlined,
    FullscreenExitOutlined,
    UnorderedListOutlined,
  } from '@vicons/antd';
  import { VolumeDownFilled, VolumeMuteFilled, VolumeUpFilled } from '@vicons/material';
  import { emit, listen } from '@tauri-apps/api/event';
  import type { MyFile } from '@/api/types/file';
  import { saveVideoHistory, videoHistory, videoPlayUrl, videoSubtitle } from '@/api/video';
  import { fileList } from '@/api/file';
  import type { SubtitleItem } from '@/api/types/video';
  import { type SelectOption } from 'naive-ui';
  import CustomLoader from './customLoader';
  import type { VideoURL } from '@/api/types/video';
  import VideoListDrawer from './components/VideoListDrawer/VideoListDrawer.vue';
  import SubtitleLayer from './components/SubtitleLayer/SubtitleLayer.vue';
  import { useSettingStore } from '@/store/setting';
  import { motion } from 'motion-v';

  const settingStore = useSettingStore();
  const { height } = useWindowSize();
  const message = useMessage();
  const videoContainer = ref<HTMLElement | null>(null);
  const videoRef = ref<HTMLVideoElement | null>(null);
  const controlsRef = ref<HTMLElement | null>(null);
  const isHovered = useElementHover(controlsRef);
  const progressBarRef = ref<HTMLElement | null>(null);
  const { playing, currentTime, duration, volume, muted, rate, seeking, waiting, ended } =
    useMediaControls(videoRef);
  const firstLoaded = ref(true);
  const controlsVisible = ref(true);
  const { start: startControlsHideTimer, stop: stopControlsHideTimer } = useTimeoutFn(() => {
    controlsVisible.value = false;
  }, 3000);
  const isFullscreen = ref(false);
  const resolutions = ref<SelectOption[]>([]);
  const currentResolution = ref<number>(0);
  const currentResolutionLabel = computed(() => {
    const resolution = resolutions.value.find((res) => res.value === currentResolution.value);
    return resolution ? resolution.label : '';
  });
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
  let hls: Hls | null = null;
  const file = ref<MyFile | null>(null);
  const pickCode = ref('');
  const videoList = ref<MyFile[]>([]);
  const videoUrlList = ref<VideoURL[]>([]);
  const historyTime = ref(0);
  const videoListShow = ref(false);
  const isDraggingProgress = ref(false);

  // 字幕相关
  const subtitleLayerRef = useTemplateRef('subtitleLayerRef');
  const subtitleList = ref<SubtitleItem[]>([]);
  const currentSubtitleSid = ref<string>();
  const subtitleEnabled = ref(false);

  // HLS 错误恢复相关
  const MAX_RECOVERY_ATTEMPTS = 3;
  let networkRecoveryAttempts = 0;
  let mediaRecoveryAttempts = 0;

  // 缓冲进度
  const bufferedProgress = computed(() => {
    if (!videoRef.value || !duration.value) return 0;
    const buffered = videoRef.value.buffered;
    if (buffered.length === 0) return 0;
    return (buffered.end(buffered.length - 1) / duration.value) * 100;
  });

  // 播放进度百分比
  const progress = computed(() => {
    return (currentTime.value / duration.value) * 100 || 0;
  });

  // 当前视频索引
  const currentVideoIndex = computed(() => {
    return videoList.value.findIndex((item) => item.pc === file.value?.pc);
  });

  // 是否有上一个/下一个视频
  const hasPreviousVideo = computed(() => currentVideoIndex.value > 0);
  const hasNextVideo = computed(
    () => videoList.value.length > 0 && currentVideoIndex.value < videoList.value.length - 1,
  );

  // 当前 Tauri 窗口实例
  const appWindow = getCurrentWebviewWindow();

  // 音量百分比
  const volumeLevel = computed({
    get: () => Math.round(volume.value * 100),
    set: (val: number) => {
      volume.value = val / 100;
    },
  });

  const unlisten = listen('set-video-list', async (event) => {
    file.value = event.payload as MyFile;
    pickCode.value = file.value.pc;
    await changeVideoUrl();
    getFileList();
  });

  let unlistenCloseRequested: (() => void) | null = null;

  onMounted(async () => {
    emit('get-video-list');
    volume.value = settingStore.videoPlayerSetting.defaultVolume;

    // 监听 Tauri 窗口关闭事件，确保资源正确清理
    unlistenCloseRequested = await appWindow.onCloseRequested(async () => {
      // 保存当前播放进度
      if (file.value && settingStore.videoPlayerSetting.isHistory && currentTime.value > 0) {
        try {
          await saveVideoHistory({
            pick_code: file.value.pc,
            time: Math.floor(currentTime.value),
          });
        } catch {
          // 窗口关闭时请求可能失败，忽略
        }
      }
      destroyHls();
    });
  });

  onBeforeUnmount(() => {
    destroyHls();
    // 清理事件监听器，忽略资源已失效的错误
    unlisten.then((f) => f()).catch(() => {});
    if (unlistenCloseRequested) {
      try {
        unlistenCloseRequested();
      } catch {
        // 资源可能已失效
      }
      unlistenCloseRequested = null;
    }
  });

  // 销毁 HLS 实例
  const destroyHls = () => {
    if (hls) {
      hls.destroy();
      hls = null;
    }
    networkRecoveryAttempts = 0;
    mediaRecoveryAttempts = 0;
  };

  const handleChangeVideo = async (value: string) => {
    if (!videoRef.value) return;
    const selectedFile = videoList.value.find((item) => item.pc === value);
    if (selectedFile) {
      file.value = selectedFile;
      pickCode.value = selectedFile.pc;
      await changeVideoUrl();
    }
  };

  const getVideoPlayUrl = async () => {
    if (!file.value) return;
    const res = await videoPlayUrl({
      pick_code: file.value.pc,
    });
    videoUrlList.value = res.data.video_url;
    resolutions.value = res.data.video_url.map((item) => ({
      label: item.title,
      value: item.definition_n,
    }));
  };

  const getVideoHistory = async () => {
    if (!file.value || !settingStore.videoPlayerSetting.isHistory) return;
    const res = await videoHistory({
      pick_code: file.value.pc,
    });
    historyTime.value = res.data.time || 0;
  };

  // 获取字幕列表
  const getSubtitleList = async () => {
    if (!file.value) return;
    try {
      const res = await videoSubtitle({ pick_code: file.value.pc });
      subtitleList.value = res.data.list || [];
      // 如果有自动载入字幕，默认启用
      if (res.data.autoload) {
        currentSubtitleSid.value = res.data.autoload.sid;
        subtitleEnabled.value = true;
      } else if (subtitleList.value.length > 0) {
        currentSubtitleSid.value = subtitleList.value[0]!.sid;
        subtitleEnabled.value = false;
      } else {
        currentSubtitleSid.value = undefined;
        subtitleEnabled.value = false;
      }
    } catch (e) {
      console.warn('获取字幕列表失败', e);
      subtitleList.value = [];
      currentSubtitleSid.value = undefined;
      subtitleEnabled.value = false;
    }
  };

  // 切换字幕
  const changeSubtitle = (sid?: string) => {
    if (!sid) {
      subtitleEnabled.value = false;
      currentSubtitleSid.value = undefined;
    } else {
      currentSubtitleSid.value = sid;
      subtitleEnabled.value = true;
    }
  };

  // 字幕选项（用于 NPopselect）
  const subtitleOptions = computed(() => {
    const options: SelectOption[] = [{ label: '关闭字幕', value: '__off__' }];
    subtitleList.value.forEach((s) => {
      options.push({
        label: s.title || s.language || s.file_name || '未知字幕',
        value: s.sid,
      });
    });
    return options;
  });

  const currentSubtitleValue = computed({
    get: () =>
      subtitleEnabled.value && currentSubtitleSid.value ? currentSubtitleSid.value : '__off__',
    set: (val: string) => {
      if (val === '__off__') {
        changeSubtitle();
      } else {
        changeSubtitle(val);
      }
    },
  });

  const currentSubtitleLabel = computed(() => {
    if (!subtitleEnabled.value || !currentSubtitleSid.value) return '字幕';
    const subtitle = subtitleList.value.find((s) => s.sid === currentSubtitleSid.value);
    return subtitle ? subtitle.title || subtitle.language || '字幕' : '字幕';
  });

  // 定时保存播放历史
  const { pause: pauseHistorySave, resume: resumeHistorySave } = useIntervalFn(
    () => {
      if (!videoRef.value || !file.value || !settingStore.videoPlayerSetting.isHistory) return;
      saveVideoHistory({
        pick_code: file.value.pc,
        time: Math.floor(currentTime.value),
      }).send();
    },
    5000,
    { immediate: false },
  );

  watch(playing, (val) => {
    if (val) {
      resumeHistorySave();
    } else {
      pauseHistorySave();
    }
  });

  // 播放结束时保存历史并尝试播放下一个
  watch(ended, (val) => {
    if (!val || !file.value) return;
    if (settingStore.videoPlayerSetting.isHistory) {
      saveVideoHistory({
        pick_code: file.value.pc,
        watch_end: 1,
      }).send();
    }
    // 自动播放下一个视频
    if (hasNextVideo.value && settingStore.videoPlayerSetting.autoPlay) {
      handleNextVideo();
    }
  });

  // 持久化音量设置
  watch(volume, (val) => {
    settingStore.videoPlayerSetting.defaultVolume = val;
  });

  // 迭代式获取文件列表，避免递归调用栈溢出
  const getFileList = async () => {
    let offset = 0;
    const limit = 1150;
    videoList.value = [];

    while (true) {
      const res = await fileList({
        cid: file.value?.pid,
        show_dir: 0,
        offset,
        type: 4,
        limit,
        cur: 1,
      });
      videoList.value.push(...res.data);
      if (videoList.value.length >= res.count) break;
      offset += limit;
    }
  };

  // 格式化时间显示
  const formatTime = (time: number): string => {
    if (!Number.isFinite(time) || time < 0) return '00:00:00';
    const hours = Math.floor(time / 3600);
    const minutes = Math.floor((time % 3600) / 60);
    const seconds = Math.floor(time % 60);
    return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
  };

  // 加载视频
  const loadVideo = (url: string, seekTime?: number) => {
    if (!videoRef.value) return;

    waiting.value = true;
    destroyHls();

    if (!Hls.isSupported()) {
      // Tauri WebView 下一般不需要原生 HLS 回退，提示用户
      message.error('当前环境不支持 HLS 视频播放');
      waiting.value = false;
      return;
    }

    hls = new Hls({
      loader: CustomLoader,
      debug: false,
      enableWorker: false,
      // 优化分片加载
      maxBufferLength: 30,
      maxMaxBufferLength: 60,
      maxBufferSize: 60 * 1000 * 1000, // 60 MB
      maxBufferHole: 0.5,
      startPosition: seekTime ?? historyTime.value ?? -1,
      // 配置 HLS.js 内置重试机制
      fragLoadPolicy: {
        default: {
          maxTimeToFirstByteMs: 15000,
          maxLoadTimeMs: 30000,
          timeoutRetry: { maxNumRetry: 3, retryDelayMs: 1000, maxRetryDelayMs: 8000 },
          errorRetry: { maxNumRetry: 3, retryDelayMs: 1000, maxRetryDelayMs: 8000 },
        },
      },
      manifestLoadPolicy: {
        default: {
          maxTimeToFirstByteMs: 15000,
          maxLoadTimeMs: 30000,
          timeoutRetry: { maxNumRetry: 3, retryDelayMs: 1000, maxRetryDelayMs: 8000 },
          errorRetry: { maxNumRetry: 3, retryDelayMs: 1000, maxRetryDelayMs: 8000 },
        },
      },
    });

    hls.loadSource(url);
    hls.attachMedia(videoRef.value);

    hls.on(Hls.Events.MANIFEST_PARSED, () => {
      waiting.value = false;

      // 设置播放速率
      if (videoRef.value) {
        videoRef.value.playbackRate = firstLoaded.value
          ? settingStore.videoPlayerSetting.defaultRate
          : rate.value;
        firstLoaded.value = false;
      }

      // 自动播放
      if (settingStore.videoPlayerSetting.autoPlay) {
        playing.value = true;
      }
    });

    hls.on(Hls.Events.ERROR, (_event, data) => {
      console.warn('[HLS Error]', data.type, data.details, data.url, data.fatal);
      if (!data.fatal) return;

      switch (data.type) {
        case Hls.ErrorTypes.NETWORK_ERROR:
          if (networkRecoveryAttempts < MAX_RECOVERY_ATTEMPTS) {
            networkRecoveryAttempts++;
            message.warning(
              `网络错误（${data.details}），正在第 ${networkRecoveryAttempts} 次重试...`,
            );
            hls?.startLoad();
          } else {
            message.error('网络错误，重试次数已用尽，请检查网络连接');
            waiting.value = false;
          }
          break;
        case Hls.ErrorTypes.MEDIA_ERROR:
          if (mediaRecoveryAttempts < MAX_RECOVERY_ATTEMPTS) {
            mediaRecoveryAttempts++;
            message.warning(`媒体错误，正在第 ${mediaRecoveryAttempts} 次恢复...`);
            hls?.recoverMediaError();
          } else {
            message.error('媒体错误，恢复失败');
            waiting.value = false;
          }
          break;
        default:
          message.error('无法加载视频');
          waiting.value = false;
          break;
      }
    });
  };

  // 跳转到指定时间
  const seek = (time: number) => {
    if (!videoRef.value || !duration.value) return;
    videoRef.value.currentTime = Math.max(0, Math.min(time, duration.value));
  };

  // 单击/双击处理：使用计数器区分单击与双击
  let clickCount = 0;
  let clickTimer: ReturnType<typeof setTimeout> | null = null;

  const handleClick = () => {
    if (!videoRef.value || !file.value) return;
    clickCount++;
    if (clickTimer) clearTimeout(clickTimer);
    clickTimer = setTimeout(() => {
      if (clickCount === 1) {
        playing.value = !playing.value;
      }
      clickCount = 0;
    }, 250);
  };

  const handleDblClick = () => {
    // 清除单击计时器，防止双击时触发暂停
    if (clickTimer) {
      clearTimeout(clickTimer);
      clickTimer = null;
    }
    clickCount = 0;
    toggleFullscreen();
  };

  // 控制条显示/隐藏
  const showControls = () => {
    controlsVisible.value = true;
    stopControlsHideTimer();
  };

  const hideControlsDelayed = () => {
    stopControlsHideTimer();
    if (isHovered.value) return;
    startControlsHideTimer();
  };

  const handleMouseMove = () => {
    showControls();
    hideControlsDelayed();
  };
  useEventListener(videoContainer, 'mousemove', handleMouseMove);

  // 进度条点击与拖拽
  const handleProgressMouseDown = (e: MouseEvent) => {
    if (!videoRef.value || !progressBarRef.value) return;

    isDraggingProgress.value = true;
    const wasPlaying = playing.value;
    if (wasPlaying) playing.value = false;

    const seekToPosition = (clientX: number) => {
      const rect = progressBarRef.value!.getBoundingClientRect();
      const position = Math.max(0, Math.min((clientX - rect.left) / rect.width, 1));
      seek(position * duration.value);
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

  // Tooltip 相关
  const showTooltip = ref(false);
  const tooltipX = ref(0);
  const hoverTime = ref(0);

  const handleProgressHover = (e: MouseEvent) => {
    if (!progressBarRef.value || !duration.value) return;
    const rect = progressBarRef.value.getBoundingClientRect();
    const position = Math.max(0, Math.min((e.clientX - rect.left) / rect.width, 1));
    hoverTime.value = position * duration.value;
    tooltipX.value = e.clientX;
  };

  // 键盘快捷键（带累加跳转）
  const keyPressInterval = 300;
  const arrowLeftCount = ref(0);
  const arrowRightCount = ref(0);
  const skipSeconds = 5;

  const { start: resetArrowLeftCount } = useTimeoutFn(() => {
    if (arrowLeftCount.value > 0) {
      seek(currentTime.value - skipSeconds * arrowLeftCount.value);
      arrowLeftCount.value = 0;
    }
  }, keyPressInterval);

  const { start: resetArrowRightCount } = useTimeoutFn(() => {
    if (arrowRightCount.value > 0) {
      seek(currentTime.value + skipSeconds * arrowRightCount.value);
      arrowRightCount.value = 0;
    }
  }, keyPressInterval);

  const {
    escape,
    arrowLeft,
    arrowRight,
    arrowUp,
    arrowDown,
    space,
    m: mKey,
  } = useMagicKeys({
    passive: false,
    onEventFired(e) {
      if (['Space', 'ArrowUp', 'ArrowDown'].includes(e.code)) {
        e.preventDefault();
      }
    },
  });

  // Esc 退出全屏
  watch(escape!, (pressed) => {
    if (pressed && isFullscreen.value) toggleFullscreen();
  });

  // 空格切换播放/暂停
  watch(space!, (pressed) => {
    if (pressed && videoRef.value) playing.value = !playing.value;
  });

  // 左方向键后退
  watch(arrowLeft!, (pressed) => {
    if (pressed && videoRef.value) {
      arrowLeftCount.value++;
      resetArrowLeftCount();
    }
  });

  // 右方向键前进
  watch(arrowRight!, (pressed) => {
    if (pressed && videoRef.value) {
      arrowRightCount.value++;
      resetArrowRightCount();
    }
  });

  // 上方向键增加音量
  watch(arrowUp!, (pressed) => {
    if (pressed) changeVolume(Math.min(volumeLevel.value + 5, 100));
  });

  // 下方向键减小音量
  watch(arrowDown!, (pressed) => {
    if (pressed) changeVolume(Math.max(volumeLevel.value - 5, 0));
  });

  // M 键静音切换
  watch(mKey!, (pressed) => {
    if (pressed) toggleMute();
  });

  // 调整音量
  const changeVolume = (val: number) => {
    volumeLevel.value = val;
    muted.value = val === 0;
  };

  // 静音切换
  const toggleMute = () => {
    muted.value = !muted.value;
  };

  // 切换分辨率（保持当前进度和播放状态）
  const changeResolution = (value: number) => {
    const resolution = videoUrlList.value.find((res) => res.definition_n === value);
    if (resolution) {
      const currentPlaybackTime = currentTime.value;
      const wasPlaying = playing.value;
      loadVideo(resolution.url, currentPlaybackTime);
      // 恢复播放状态
      if (wasPlaying) {
        const unwatch = watch(waiting, (isWaiting) => {
          if (!isWaiting) {
            playing.value = true;
            unwatch();
          }
        });
      }
    }
  };

  // 切换播放速度
  const changePlaybackSpeed = (speed: number) => {
    if (videoRef.value) {
      rate.value = speed;
      videoRef.value.playbackRate = speed;
    }
  };

  // 全屏切换（使用 Tauri 窗口 API）
  const toggleFullscreen = async () => {
    try {
      const isCurrentlyFullscreen = await appWindow.isFullscreen();
      await appWindow.setFullscreen(!isCurrentlyFullscreen);
      isFullscreen.value = !isCurrentlyFullscreen;
    } catch (err) {
      console.error('全屏切换失败:', err);
      message.error('无法切换全屏模式');
    }
  };

  const handleNextVideo = async () => {
    if (!hasNextVideo.value) {
      message.warning('已经是最后一个视频了');
      return;
    }
    const nextVideo = videoList.value[currentVideoIndex.value + 1];
    if (!nextVideo) return;
    file.value = nextVideo;
    pickCode.value = nextVideo.pc;
    await changeVideoUrl();
  };

  const handlePreviousVideo = async () => {
    if (!hasPreviousVideo.value) {
      message.warning('已经是第一个视频了');
      return;
    }
    const previousVideo = videoList.value[currentVideoIndex.value - 1];
    if (!previousVideo) return;
    file.value = previousVideo;
    pickCode.value = previousVideo.pc;
    await changeVideoUrl();
  };

  const changeVideoUrl = async () => {
    pauseHistorySave();
    await getVideoPlayUrl();
    await Promise.all([getVideoHistory(), getSubtitleList()]);
    if (videoUrlList.value.length === 0) {
      message.error('无可用的视频源');
      return;
    }
    const highestResolution = videoUrlList.value.reduce((prev, current) =>
      prev.definition_n > current.definition_n ? prev : current,
    );
    currentResolution.value = highestResolution.definition_n;
    loadVideo(highestResolution.url);

    // 加载字幕
    await nextTick();
    if (subtitleEnabled.value) {
      subtitleLayerRef.value?.loadSubtitle();
    }

    // 更新 Tauri 窗口标题
    if (file.value) {
      appWindow.setTitle(file.value.fn).catch(() => {});
    }
  };
</script>
