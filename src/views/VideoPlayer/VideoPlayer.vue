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
          v-motion
          :animate="videoRotationAnimate"
          :transition="{ duration: 0.3 }"
          class="w-full h-full object-contain cursor-pointer"
          :class="{ 'cursor-none!': !controlsVisible && playing }"
          @click="handleClick"
          @dblclick="handleDblClick"
        ></video>
        <!-- 视频加载中 -->
        <AnimatePresence>
          <div
            v-if="showLoading"
            key="loading"
            v-motion
            :animate="{ opacity: 1 }"
            :initial="{ opacity: 0 }"
            :exit="{ opacity: 0 }"
            :transition="{ duration: 0.3 }"
            class="absolute inset-0 flex flex-col justify-center items-center gap-4 bg-black/50 text-white z-10 pointer-events-none"
          >
            <NSpin size="large" />
            <span class="text-sm">加载中...</span>
          </div>
        </AnimatePresence>
        <!-- 视频标题 -->
        <AnimatePresence>
          <div
            v-if="controlsVisible && file"
            key="title"
            v-motion
            :animate="{ opacity: 1, y: 0 }"
            :initial="{ opacity: 0, y: -20 }"
            :exit="{ opacity: 0, y: -20 }"
            class="absolute top-0 left-0 w-full px-4 py-3 bg-linear-to-b from-black/80 to-transparent z-20 pointer-events-none"
          >
            <NEllipsis class="text-white text-sm font-medium">
              {{ file.fn }}
            </NEllipsis>
          </div>
        </AnimatePresence>
        <!-- 居中提示 -->
        <CenterIndicator ref="centerIndicatorRef" />
        <!-- 字幕显示层 -->
        <SubtitleLayer
          ref="subtitleLayerRef"
          :subtitle-list="subtitleList"
          :current-sid="currentSubtitleSid"
          :enabled="subtitleEnabled"
          :current-time="currentTime"
          :video-element="videoRef"
        />
        <!-- 视频控制条 -->
        <AnimatePresence>
          <div
            v-if="controlsVisible"
            key="controls"
            ref="controlsRef"
            v-motion
            :animate="{ opacity: 1, y: 0 }"
            :initial="{ opacity: 0, y: 20 }"
            :exit="{ opacity: 0, y: 20 }"
            class="absolute bottom-0 left-0 w-full px-4 py-2 bg-linear-to-t from-black/90 to-transparent z-20 box-border"
          >
            <ControlBar
              v-model:playing="playing"
              v-model:muted="muted"
              v-model:volume-level="volumeLevel"
              v-model:rate="rate"
              v-model:current-resolution="currentResolution"
              v-model:current-subtitle-value="currentSubtitleValue"
              v-model:video-list-show="videoListShow"
              :current-time="currentTime"
              :duration="duration"
              :progress="progress"
              :is-fullscreen="isFullscreen"
              :has-previous-video="hasPreviousVideo"
              :has-next-video="hasNextVideo"
              :resolutions="resolutions"
              :subtitle-list="subtitleList"
              :video-list-length="videoList.length"
              @previous-video="handlePreviousVideo"
              @next-video="handleNextVideo"
              @toggle-fullscreen="toggleFullscreen"
              @change-resolution="changeResolution"
              @change-playback-speed="changePlaybackSpeed"
              @rotate-video="rotateVideo"
              @seek="seek"
              @toggle-play="centerIndicatorRef?.show(playing ? 'play' : 'pause')"
              @toggle-mute="centerIndicatorRef?.show(muted ? 'mute' : 'volume', volumeLevel)"
              @change-volume="
                (v: number) => centerIndicatorRef?.show(v === 0 ? 'mute' : 'volume', v)
              "
            />
          </div>
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
  import { emit, listen } from '@tauri-apps/api/event';
  import type { MyFile } from '@/api/types/file';
  import { saveVideoHistory, videoHistory, videoPlayUrl, videoSubtitle } from '@/api/video';
  import { fileList } from '@/api/file';
  import type { SubtitleItem } from '@/api/types/video';
  import { type SelectOption } from 'naive-ui';
  import CustomLoader from './customLoader';
  import type { VideoURL } from '@/api/types/video';
  import CenterIndicator from './components/CenterIndicator/CenterIndicator.vue';
  import ControlBar from './components/ControlBar/ControlBar.vue';
  import VideoListDrawer from './components/VideoListDrawer/VideoListDrawer.vue';
  import SubtitleLayer from './components/SubtitleLayer/SubtitleLayer.vue';
  import { useSettingStore } from '@/store/setting';
  import { vMotion } from 'motion-v';

  const settingStore = useSettingStore();
  const message = useMessage();
  const videoContainer = ref<HTMLElement | null>(null);
  const videoRef = ref<HTMLVideoElement | null>(null);
  const controlsRef = ref<HTMLElement | null>(null);
  const isHovered = useElementHover(controlsRef);
  const { playing, currentTime, duration, volume, muted, rate, seeking, waiting, ended } =
    useMediaControls(videoRef);

  // 加载遮罩延迟 300ms 显示（类似 YouTube，避免快速缓冲闪烁，同时给居中提示留出展示时间）
  const showLoading = ref(false);
  const { start: startLoadingTimer, stop: stopLoadingTimer } = useTimeoutFn(
    () => {
      showLoading.value = true;
    },
    300,
    { immediate: false },
  );
  watch(
    () => waiting.value || seeking.value,
    (isLoading) => {
      if (isLoading) {
        startLoadingTimer();
      } else {
        stopLoadingTimer();
        showLoading.value = false;
      }
    },
  );

  const firstLoaded = ref(true);
  const controlsVisible = ref(true);
  const { start: startControlsHideTimer, stop: stopControlsHideTimer } = useTimeoutFn(() => {
    controlsVisible.value = false;
  }, 3000);
  const isFullscreen = ref(false);
  const resolutions = ref<SelectOption[]>([]);
  const currentResolution = ref<number>(0);
  let hls: Hls | null = null;
  const file = ref<MyFile | null>(null);
  const pickCode = ref('');
  const videoList = ref<MyFile[]>([]);
  const videoUrlList = ref<VideoURL[]>([]);
  const historyTime = ref(0);
  const videoListShow = ref(false);

  // 视频旋转相关（使用 motion-v 动画库）
  // rotation 持续累加（0, 90, 180, ...），motion 始终正向插值，无需 wrap-around hack
  const rotation = ref(0);
  const videoRotationAnimate = computed(() => {
    const deg = rotation.value;
    if (deg === 0) return { rotate: 0, scale: 1 };

    const effectiveDeg = ((deg % 360) + 360) % 360;
    const isVertical = effectiveDeg % 180 !== 0;
    const container = videoContainer.value;
    const video = videoRef.value;

    if (!isVertical || !container) {
      return { rotate: deg, scale: 1 };
    }

    const cW = container.clientWidth;
    const cH = container.clientHeight;

    let scaleVal: number;
    if (video && video.videoWidth > 0 && video.videoHeight > 0) {
      const vW = video.videoWidth;
      const vH = video.videoHeight;
      const fitScale = Math.min(cW / vW, cH / vH);
      const renderedW = vW * fitScale;
      const renderedH = vH * fitScale;
      scaleVal = Math.min(cW / renderedH, cH / renderedW);
    } else {
      scaleVal = cH / cW;
    }

    return { rotate: deg, scale: scaleVal };
  });

  const rotateVideo = () => {
    rotation.value += 90;
  };

  // 居中提示
  const centerIndicatorRef = useTemplateRef('centerIndicatorRef');

  // 字幕相关
  const subtitleLayerRef = useTemplateRef('subtitleLayerRef');
  const subtitleList = ref<SubtitleItem[]>([]);
  const currentSubtitleSid = ref<string>();
  const subtitleEnabled = ref(false);

  // HLS 错误恢复相关
  const MAX_RECOVERY_ATTEMPTS = 3;
  let networkRecoveryAttempts = 0;
  let mediaRecoveryAttempts = 0;

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
      // 根据设置决定是否默认启用字幕
      const shouldEnable = settingStore.subtitleStyleSetting.defaultEnabled;
      if (res.data.autoload) {
        currentSubtitleSid.value = res.data.autoload.sid;
        subtitleEnabled.value = shouldEnable;
      } else if (subtitleList.value.length > 0) {
        currentSubtitleSid.value = subtitleList.value[0]!.sid;
        subtitleEnabled.value = shouldEnable;
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
  const { start: startClickTimer, stop: stopClickTimer } = useTimeoutFn(
    () => {
      if (clickCount === 1) {
        playing.value = !playing.value;
        centerIndicatorRef.value?.show(playing.value ? 'play' : 'pause');
      }
      clickCount = 0;
    },
    250,
    { immediate: false },
  );

  const handleClick = () => {
    if (!videoRef.value || !file.value) return;
    clickCount++;
    stopClickTimer();
    startClickTimer();
  };

  const handleDblClick = () => {
    stopClickTimer();
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

  // 键盘快捷键（带累加跳转）
  const keyPressInterval = 300;
  const arrowLeftCount = ref(0);
  const arrowRightCount = ref(0);
  const skipSeconds = 5;

  const { start: resetArrowLeftCount } = useTimeoutFn(() => {
    if (arrowLeftCount.value > 0) {
      centerIndicatorRef.value?.show('backward', skipSeconds * arrowLeftCount.value);
      seek(currentTime.value - skipSeconds * arrowLeftCount.value);
      arrowLeftCount.value = 0;
    }
  }, keyPressInterval);

  const { start: resetArrowRightCount } = useTimeoutFn(() => {
    if (arrowRightCount.value > 0) {
      centerIndicatorRef.value?.show('forward', skipSeconds * arrowRightCount.value);
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
    if (pressed && videoRef.value) {
      playing.value = !playing.value;
      centerIndicatorRef.value?.show(playing.value ? 'play' : 'pause');
    }
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
    if (pressed) {
      const newVol = Math.min(volumeLevel.value + 5, 100);
      changeVolume(newVol);
      centerIndicatorRef.value?.show('volume', newVol);
    }
  });

  // 下方向键减小音量
  watch(arrowDown!, (pressed) => {
    if (pressed) {
      const newVol = Math.max(volumeLevel.value - 5, 0);
      changeVolume(newVol);
      centerIndicatorRef.value?.show(newVol === 0 ? 'mute' : 'volume', newVol);
    }
  });

  // M 键静音切换
  watch(mKey!, (pressed) => {
    if (pressed) {
      toggleMute();
      centerIndicatorRef.value?.show(muted.value ? 'mute' : 'volume', volumeLevel.value);
    }
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
      centerIndicatorRef.value?.show('speed', speed);
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
