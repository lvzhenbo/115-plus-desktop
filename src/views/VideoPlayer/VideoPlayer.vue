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
          @dblclick="toggleFullscreen"
        ></video>
        <!-- 视频加载中 -->
        <div
          v-if="waiting || seeking"
          class="absolute inset-0 flex flex-col justify-center items-center gap-4 bg-black/70 text-white z-10"
        >
          <NSpin size="large" />
          <span>加载中...</span>
        </div>
        <!-- 视频控制条 -->
        <div
          v-show="controlsVisible"
          class="absolute bottom-0 left-0 w-full px-4 py-2 bg-gradient-to-t from-black/80 to-transparent transition duration-300 z-20"
          @mouseenter="showControls"
          @mouseleave="hideControlsDelayed"
        >
          <div class="flex items-center mb-2">
            <NTooltip :show="showTooltip" :x="tooltipX" :y="height - 70" placement="top">
              {{ formatTime(hoverTime) }}
            </NTooltip>
            <div
              ref="progressBarRef"
              class="flex-1 h-2 bg-white/30 rounded cursor-pointer relative"
              @click="handleSeek"
              @mousemove="handleProgressHover"
              @mouseenter="showTooltip = true"
              @mouseleave="showTooltip = false"
            >
              <div
                class="h-full bg-[#18a058] rounded absolute top-0 left-0"
                :style="{ width: `${progress}%` }"
              ></div>
              <div
                class="h-4 w-4 rounded-full bg-[#18a058] absolute top-1/2 -translate-y-1/2 -ml-2"
                :style="{ left: `${progress}%` }"
              ></div>
            </div>
            <div class="ml-4 text-white text-sm min-w-[100px] md:min-w-[100px] text-right">
              {{ formatTime(currentTime) }} / {{ formatTime(duration) }}
            </div>
          </div>
          <!-- 控制栏 -->
          <div class="flex items-center">
            <!-- 控制栏左侧 -->
            <div class="flex items-center gap-2">
              <NButton quaternary circle :disabled="!videoList.length" @click="handlePreviousVideo">
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
              <NButton quaternary circle :disabled="!videoList.length" @click="handleNextVideo">
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
            <div class="flex items-center ml-auto gap-2">
              <!-- 分辨率选择 -->
              <NPopselect v-model:value="currentResolution" :options="resolutions">
                <NButton quaternary round> {{ currentResolutionLabel }} </NButton>
              </NPopselect>
              <!-- 播放速度选择 -->
              <NPopselect
                v-model:value="rate"
                :options="playbackSpeeds"
                @update:value="changePlaybackSpeed"
              >
                <NButton quaternary circle> {{ rate }}x </NButton>
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
        </div>
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
  import { Window } from '@tauri-apps/api/window';
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
  import { saveVideoHistory, videoHistory, videoPlayUrl } from '@/api/video';
  import { fileList } from '@/api/file';
  import { type SelectOption } from 'naive-ui';
  import CustomLoader from './customLoader';
  import type { VideoURL } from '@/api/types/video';
  import VideoListDrawer from './components/VideoListDrawer/VideoListDrawer.vue';

  // interface Resolution {
  //   height: number;
  //   width?: number;
  //   bitrate?: number;
  //   url: string;
  //   level?: number;
  //   label: string;
  // }

  const { height } = useWindowSize();
  const message = useMessage();
  const videoContainer = ref<HTMLElement | null>(null);
  const videoRef = ref<HTMLVideoElement | null>(null);
  const progressBarRef = ref<HTMLElement | null>(null);
  const { playing, currentTime, duration, volume, muted, rate, seeking, waiting, ended } =
    useMediaControls(videoRef);
  const controlsVisible = ref<boolean>(true);
  const { start: startControlsHideTimer, stop: stopControlsHideTimer } = useTimeoutFn(() => {
    controlsVisible.value = false;
  }, 3000);
  const isFullscreen = ref<boolean>(false);
  const resolutions = ref<SelectOption[]>([]);
  const currentResolution = ref<number>(0);
  const currentResolutionLabel = computed(() => {
    const resolution = resolutions.value.find((res) => res.value === currentResolution.value);
    return resolution ? resolution.label : '';
  });
  // const resolutions = ref<Resolution[]>([]); // 可用的分辨率列表
  // const currentResolution = ref<Resolution | null>(null); // 当前选择的分辨率
  // const currentResolutionLabel = ref<string>(''); // 当前选择的分辨率标签
  // const showResolutionMenu = ref<boolean>(false); // 是否显示分辨率菜单
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
  const pickCode = ref<string>('');
  const videoList = ref<MyFile[]>([]);
  const videoUrlList = ref<VideoURL[]>([]);
  const historyTime = ref<number>(0);
  const videoListShow = ref<boolean>(false);
  // 计算进度百分比
  const progress = computed(() => {
    return (currentTime.value / duration.value) * 100 || 0;
  });
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
    getFileList(0);
  });

  onMounted(async () => {
    emit('get-video-list');

    // 设置初始播放速度
    if (videoRef.value) {
      rate.value = 1;
    }
  });

  onBeforeUnmount(() => {
    // 销毁HLS实例
    if (hls) {
      hls.destroy();
    }

    unlisten.then((f) => f());
  });

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
    resolutions.value = res.data.video_url.map((item) => {
      return {
        label: item.title,
        value: item.definition_n,
      };
    });
  };

  const getVideoHistory = async () => {
    if (!file.value) return;
    const res = await videoHistory({
      pick_code: file.value.pc,
    });
    historyTime.value = res.data.time;
  };

  const { pause, resume } = useIntervalFn(
    () => {
      if (!videoRef.value) return;
      if (!file.value) return;
      saveVideoHistory({
        pick_code: file.value.pc,
        time: currentTime.value,
      }).send();
    },
    5000,
    { immediate: false },
  );

  watch(playing, (val) => {
    if (val) {
      resume();
    } else {
      pause();
    }
  });

  watch(ended, (val) => {
    if (val) {
      if (!file.value) return;
      saveVideoHistory({
        pick_code: file.value.pc,
        watch_end: 1,
      }).send();
    }
  });

  const getFileList = async (offset: number) => {
    const res = await fileList({
      cid: file.value?.pid,
      show_dir: 0,
      offset,
      type: 4,
      limit: 1150,
      cur: 1,
    });
    if (offset === 0) {
      videoList.value = [];
    }
    videoList.value = [...videoList.value, ...res.data];
    if (videoList.value.length < res.count) {
      getFileList(offset + 1150);
    }
  };

  // 格式化时间为 HH:MM:SS 格式，始终显示小时部分
  const formatTime = (time: number): string => {
    const hours = Math.floor(time / 3600);
    const minutes = Math.floor((time % 3600) / 60);
    const seconds = Math.floor(time % 60);

    return `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
  };

  // 加载视频
  const loadVideo = (url: string, seekTime?: number) => {
    if (!videoRef.value) return;

    waiting.value = true;

    // 清理现有HLS实例
    if (hls) {
      hls.destroy();
      hls = null;
    }

    // 使用HLS.js加载m3u8视频
    if (Hls.isSupported()) {
      hls = new Hls({
        loader: CustomLoader,
        debug: false,
        enableWorker: false,
      });
      hls.loadSource(url);
      hls.attachMedia(videoRef.value);

      hls.on(Hls.Events.MANIFEST_PARSED, (_event, _data) => {
        waiting.value = false;

        // 获取可用分辨率列表
        // const levels = hls?.levels || [];
        // if (levels.length > 0) {
        //   resolutions.value = levels
        //     .map((level, index) => {
        //       let label: string;
        //       if (level.height >= 1080) {
        //         label = '1080p 高清';
        //       } else if (level.height >= 720) {
        //         label = '720p 高清';
        //       } else if (level.height >= 480) {
        //         label = '480p 标清';
        //       } else if (level.height >= 360) {
        //         label = '360p 流畅';
        //       } else {
        //         label = `${level.height}p`;
        //       }
        //       return {
        //         height: level.height,
        //         width: level.width,
        //         bitrate: level.bitrate,
        //         url: level.url[0],
        //         level: index,
        //         label,
        //       };
        //     })
        //     .sort((a, b) => b.height - a.height); // 按分辨率高度降序排序

        //   // 默认选择最高画质
        //   if (resolutions.value.length > 0) {
        //     const highestRes = resolutions.value[0];
        //     if (hls && highestRes.level !== undefined) {
        //       hls.currentLevel = highestRes.level;
        //     }
        //     currentResolution.value = highestRes;
        //     currentResolutionLabel.value = highestRes.label;
        //   }
        // }

        // 尝试自动播放
        playing.value = true;
        if (seekTime) {
          seek(seekTime);
        }
        if (videoRef.value) {
          videoRef.value.playbackRate = rate.value;
          videoRef.value.currentTime = seekTime || historyTime.value;
        }
      });

      // hls.on(Hls.Events.LEVEL_SWITCHED, (_event, data) => {
      //   const { level } = data;
      //   const newResolution = resolutions.value.find((res) => res.level === level);
      //   if (newResolution) {
      //     currentResolution.value = newResolution;
      //     currentResolutionLabel.value = newResolution.label;
      //   }
      // });

      hls.on(Hls.Events.ERROR, (_event, data) => {
        if (data.fatal) {
          switch (data.type) {
            case Hls.ErrorTypes.NETWORK_ERROR:
              message.error('网络错误，尝试恢复...');
              hls?.startLoad();
              break;
            case Hls.ErrorTypes.MEDIA_ERROR:
              message.error('媒体错误，尝试恢复...');
              hls?.recoverMediaError();
              break;
            default:
              message.error('无法加载视频，请检查URL是否正确');
              waiting.value = false;
              break;
          }
        }
      });
    } else if (videoRef.value.canPlayType('application/vnd.apple.mpegurl')) {
      // 对于原生支持HLS的浏览器（如Safari）
      videoRef.value.src = url;
      videoRef.value.addEventListener('loadedmetadata', () => {
        waiting.value = false;
        playing.value = true;
        if (seekTime) {
          seek(seekTime);
        }
        if (videoRef.value) {
          videoRef.value.playbackRate = rate.value;
          videoRef.value.currentTime = seekTime || historyTime.value;
        }
      });
    } else {
      message.error('您的浏览器不支持HLS视频播放');
      waiting.value = false;
    }
  };

  // 更改视频播放位置
  const seek = (time: number) => {
    if (!videoRef.value) return;
    videoRef.value.currentTime = time;
  };

  // 单击事件处理
  const handleClick = () => {
    if (!videoRef.value || !file.value) return;

    // 延迟执行播放/暂停切换，避免与双击冲突
    setTimeout(() => {
      if (!videoRef.value) return;
      playing.value = !playing.value;
    }, 200);
  };

  // 显示控制条
  const showControls = () => {
    controlsVisible.value = true;
    stopControlsHideTimer();
  };

  // 延迟隐藏控制条
  const hideControlsDelayed = () => {
    stopControlsHideTimer();
    startControlsHideTimer();
  };

  // 在鼠标移动时显示控制条
  const handleMouseMove = () => {
    showControls();
    hideControlsDelayed();
  };
  useEventListener(videoContainer, 'mousemove', handleMouseMove);

  // 拖动进度条调整位置
  const handleSeek = (e: MouseEvent) => {
    if (!videoRef.value) return;

    const progressBar = e.currentTarget as HTMLElement;
    const rect = progressBar.getBoundingClientRect();
    const clickPosition = (e.clientX - rect.left) / rect.width;
    const newTime = clickPosition * duration.value;

    seek(newTime);
  };

  // 添加tooltip相关变量
  const showTooltip = ref<boolean>(false);
  const tooltipX = ref<number>(0);
  const hoverTime = ref<number>(0);

  // 处理进度条上的鼠标移动，计算并显示对应时间点
  const handleProgressHover = (e: MouseEvent) => {
    if (!progressBarRef.value || !duration.value) return;

    const rect = progressBarRef.value.getBoundingClientRect();
    const position = (e.clientX - rect.left) / rect.width;
    const time = Math.max(0, Math.min(position * duration.value, duration.value));

    hoverTime.value = time;
    tooltipX.value = e.clientX;
  };

  // 为键盘快捷键添加累加功能相关变量
  const keyPressInterval = 300; // 按键检测间隔，单位毫秒
  const arrowLeftCount = ref(0); // 左键按下次数
  const arrowRightCount = ref(0); // 右键按下次数
  const skipSeconds = 5; // 基础跳转秒数

  const { start: resetArrowLeftCount } = useTimeoutFn(() => {
    if (arrowLeftCount.value > 0) {
      // 累加后退时间 = 基础跳转秒数 × 按键次数
      seek(currentTime.value - skipSeconds * arrowLeftCount.value);
      arrowLeftCount.value = 0;
    }
  }, keyPressInterval);

  const { start: resetArrowRightCount } = useTimeoutFn(() => {
    if (arrowRightCount.value > 0) {
      // 累加前进时间 = 基础跳转秒数 × 按键次数
      seek(currentTime.value + skipSeconds * arrowRightCount.value);
      arrowRightCount.value = 0;
    }
  }, keyPressInterval);

  // 使用useMagicKeys实现键盘快捷键控制
  const { escape, arrowLeft, arrowRight, space } = useMagicKeys({
    passive: false,
    onEventFired(e) {
      if (e.code === 'Space') {
        e.preventDefault();
      }
    },
  });

  // 监听Esc键退出全屏
  watch(escape, (pressed) => {
    if (pressed && isFullscreen.value) {
      toggleFullscreen();
    }
  });

  // 监听空格键切换播放/暂停
  watch(space, (pressed) => {
    if (pressed && videoRef.value) {
      playing.value = !playing.value;
    }
  });

  // 监听左方向键后退累加
  watch(arrowLeft, (pressed) => {
    if (pressed && videoRef.value) {
      arrowLeftCount.value++;
      resetArrowLeftCount();
    }
  });

  // 监听右方向键前进累加
  watch(arrowRight, (pressed) => {
    if (pressed && videoRef.value) {
      arrowRightCount.value++;
      resetArrowRightCount();
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

  watch(currentResolution, (val) => {
    changeResolution(val);
  });

  // 切换分辨率
  const changeResolution = (value: number) => {
    // const resolution = resolutions.value.find((res) => res.label === label);
    // if (resolution && hls) {
    //   hls.currentLevel = resolution.level!;
    //   currentResolution.value = resolution;
    //   currentResolutionLabel.value = resolution.label;
    // }
    const resolution = videoUrlList.value.find((res) => res.definition_n === value);
    if (resolution) {
      // 保存当前播放进度和播放状态
      const currentPlaybackTime = currentTime.value;

      // 加载新的视频源，同时传递当前播放进度
      loadVideo(resolution.url, currentPlaybackTime);
    }
  };

  // 切换播放速度
  const changePlaybackSpeed = (speed: number) => {
    if (videoRef.value) {
      rate.value = speed;
      videoRef.value.playbackRate = speed;
    }
  };

  // 全屏切换
  const toggleFullscreen = async () => {
    try {
      // 获取当前应用的窗口实例
      const currentWindow = Window.getCurrent();

      // 使用 Tauri 的窗口 API 实现真正的屏幕全屏
      const isCurrentlyFullscreen = await currentWindow.isFullscreen();

      if (!isCurrentlyFullscreen) {
        // 进入全屏模式
        await currentWindow.setFullscreen(true);
        isFullscreen.value = true;
      } else {
        // 退出全屏模式
        await currentWindow.setFullscreen(false);
        isFullscreen.value = false;
      }
    } catch (err) {
      console.error(err);

      // 如果 Tauri 全屏失败，回退到 Web API 的全屏
      if (!document.fullscreenElement && videoContainer.value) {
        videoContainer.value
          .requestFullscreen()
          .then(() => {
            isFullscreen.value = true;
          })
          .catch((error) => {
            message.error(`无法进入全屏模式: ${error.message}`);
          });
      } else {
        document.exitFullscreen();
        isFullscreen.value = false;
      }
    }
  };

  const handleNextVideo = async () => {
    const nextVideoIndex = videoList.value.findIndex((item) => item.pc === file.value?.pc) + 1;
    if (nextVideoIndex < videoList.value.length) {
      const nextVideo = videoList.value[nextVideoIndex];
      file.value = nextVideo;
      pickCode.value = nextVideo.pc;
      await changeVideoUrl();
    } else {
      message.warning('已经是最后一个视频了');
    }
  };

  const handlePreviousVideo = async () => {
    const previousVideoIndex = videoList.value.findIndex((item) => item.pc === file.value?.pc) - 1;
    if (previousVideoIndex >= 0) {
      const previousVideo = videoList.value[previousVideoIndex];
      file.value = previousVideo;
      pickCode.value = previousVideo.pc;
      await changeVideoUrl();
    } else {
      message.warning('已经是第一个视频了');
    }
  };

  const changeVideoUrl = async () => {
    await getVideoPlayUrl();
    await getVideoHistory();
    const highestResolution = videoUrlList.value.reduce((prev, current) => {
      return prev.definition_n > current.definition_n ? prev : current;
    });
    currentResolution.value = highestResolution.definition_n;
    loadVideo(highestResolution.url);
  };
</script>

<style scoped></style>
