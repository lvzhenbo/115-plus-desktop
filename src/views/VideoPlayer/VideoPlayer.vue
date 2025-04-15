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
            <div
              class="flex-1 h-1.5 bg-white/30 rounded cursor-pointer relative"
              @click="handleSeek"
            >
              <div
                class="h-full bg-[#18a058] rounded absolute top-0 left-0"
                :style="{ width: `${progress}%` }"
              ></div>
              <div
                class="h-3 w-3 rounded-full bg-[#18a058] absolute top-1/2 -translate-y-1/2 -ml-1.5"
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
              <NButton quaternary circle>
                <template #icon>
                  <NIcon size="24" class="text-white"><StepBackwardOutlined /></NIcon>
                </template>
              </NButton>
              <NButton quaternary circle @click="toggle">
                <template #icon>
                  <NIcon size="24" class="text-white">
                    <PauseCircleOutlined v-if="playing" />
                    <PlayCircleOutlined v-else />
                  </NIcon>
                </template>
              </NButton>
              <NButton quaternary circle>
                <template #icon>
                  <NIcon size="24" class="text-white"><StepForwardOutlined /></NIcon>
                </template>
              </NButton>
              <NButton quaternary circle @click="toggleMute">
                <template #icon>
                  <NIcon size="24" class="text-white">
                    <SoundOutlined v-if="!muted" />
                    <SoundOutlined v-else />
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
              <NPopover
                v-if="resolutions.length > 0"
                trigger="click"
                placement="top"
                :show="showResolutionMenu"
                @update:show="showResolutionMenu = $event"
              >
                <template #trigger>
                  <NButton quaternary circle @click="showResolutionMenu = true">
                    <template #icon>
                      <NIcon size="24" class="text-white"><SettingOutlined /></NIcon>
                    </template>
                  </NButton>
                </template>
                <div class="p-2 min-w-[140px]">
                  <div class="mb-2 text-sm font-bold">选择画质</div>
                  <NRadioGroup
                    v-model:value="currentResolutionLabel"
                    @update:value="changeResolution"
                  >
                    <div class="space-y-2">
                      <NRadio
                        v-for="res in resolutions"
                        :key="res.height"
                        :value="res.label"
                        :disabled="res.label === currentResolution?.label"
                      >
                        {{ res.label }}
                      </NRadio>
                    </div>
                  </NRadioGroup>
                </div>
              </NPopover>
              <!-- 播放速度选择 -->
              <NDropdown trigger="hover" :options="playbackSpeeds" @select="changePlaybackSpeed">
                <NButton quaternary circle> {{ rate }}x </NButton>
              </NDropdown>
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
  </div>
</template>

<script setup lang="ts">
  import Hls from 'hls.js';
  import { Window } from '@tauri-apps/api/window';
  import { useMediaControls, useTimeoutFn, useMagicKeys } from '@vueuse/core';
  import {
    PlayCircleOutlined,
    PauseCircleOutlined,
    StepBackwardOutlined,
    StepForwardOutlined,
    SoundOutlined,
    FullscreenOutlined,
    FullscreenExitOutlined,
    SettingOutlined,
  } from '@vicons/antd';
  import { emit, listen } from '@tauri-apps/api/event';
  import type { MyFile } from '@/api/types/file';
  import { videoPlayUrl } from '@/api/video';
  import { fileList } from '@/api/file';
  import { type DropdownOption } from 'naive-ui';
  import CustomLoader from './customLoader';

  interface Resolution {
    height: number;
    width?: number;
    bitrate?: number;
    url: string;
    level?: number;
    label: string;
  }

  const message = useMessage();
  const videoContainer = ref<HTMLElement | null>(null);
  const videoRef = ref<HTMLVideoElement | null>(null);
  const { playing, currentTime, duration, volume, muted, rate, seeking, waiting } =
    useMediaControls(videoRef);
  const controlsVisible = ref<boolean>(true);
  const isFullscreen = ref<boolean>(false);
  const resolutions = ref<Resolution[]>([]); // 可用的分辨率列表
  const currentResolution = ref<Resolution | null>(null); // 当前选择的分辨率
  const currentResolutionLabel = ref<string>(''); // 当前选择的分辨率标签
  const showResolutionMenu = ref<boolean>(false); // 是否显示分辨率菜单
  const playbackSpeeds: DropdownOption[] = [
    { label: '5x', key: 5 },
    { label: '4x', key: 4 },
    { label: '3x', key: 3 },
    { label: '2x', key: 2 },
    { label: '1.5x', key: 1.5 },
    { label: '1.25x', key: 1.25 },
    { label: '1x', key: 1 },
    { label: '0.75x', key: 0.75 },
    { label: '0.5x', key: 0.5 },
  ];
  let hls: Hls | null = null;
  const file = ref<MyFile | null>(null);
  const files = ref<MyFile[]>([]);
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
  const unlisten = listen('add-video-list', async (event) => {
    file.value = event.payload as MyFile;
    if (!file.value.pc) return;
    const res = await videoPlayUrl({
      pick_code: file.value?.pc,
    });
    loadVideo(res.data.video_url[res.data.video_url.length - 1].url);
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

  const { start: startControlsHideTimer, stop: stopControlsHideTimer } = useTimeoutFn(() => {
    controlsVisible.value = false;
  }, 3000);

  const getFileList = async (offset: number) => {
    const res = await fileList({
      cid: '0',
      show_dir: 0,
      offset,
      type: 4,
      limit: 1150,
    });
    if (offset === 0) {
      files.value = [];
    }
    files.value = [...files.value, ...res.data];
    if (files.value.length < res.count) {
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
  const loadVideo = (url: string) => {
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
        const levels = hls?.levels || [];
        if (levels.length > 0) {
          resolutions.value = levels
            .map((level, index) => {
              let label: string;
              if (level.height >= 1080) {
                label = '1080p 高清';
              } else if (level.height >= 720) {
                label = '720p 高清';
              } else if (level.height >= 480) {
                label = '480p 标清';
              } else if (level.height >= 360) {
                label = '360p 流畅';
              } else {
                label = `${level.height}p`;
              }
              return {
                height: level.height,
                width: level.width,
                bitrate: level.bitrate,
                url: level.url[0],
                level: index,
                label,
              };
            })
            .sort((a, b) => b.height - a.height); // 按分辨率高度降序排序

          // 默认选择最高画质
          if (resolutions.value.length > 0) {
            const highestRes = resolutions.value[0];
            if (hls && highestRes.level !== undefined) {
              hls.currentLevel = highestRes.level;
            }
            currentResolution.value = highestRes;
            currentResolutionLabel.value = highestRes.label;
          }
        }

        // 尝试自动播放
        play().catch(() => {
          message.warning('自动播放失败，请点击播放按钮手动播放');
        });
      });

      hls.on(Hls.Events.LEVEL_SWITCHED, (_event, data) => {
        const { level } = data;
        const newResolution = resolutions.value.find((res) => res.level === level);
        if (newResolution) {
          currentResolution.value = newResolution;
          currentResolutionLabel.value = newResolution.label;
        }
      });

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
        play().catch(() => {
          message.warning('自动播放失败，请点击播放按钮手动播放');
        });
      });
    } else {
      message.error('您的浏览器不支持HLS视频播放');
      waiting.value = false;
    }
  };

  // 播放、暂停和视频操作功能
  const play = () => {
    if (!videoRef.value) return Promise.reject('No video element');
    return videoRef.value.play();
  };

  const pause = () => {
    if (!videoRef.value) return;
    videoRef.value.pause();
  };

  const toggle = () => {
    if (playing.value) {
      pause();
    } else {
      play().catch((err) => {
        console.error('播放失败:', err);
        message.warning('播放失败，请重试');
      });
    }
  };

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
      toggle();
    }, 200);
  };

  // 拖动进度条调整位置
  const handleSeek = (e: MouseEvent) => {
    if (!videoRef.value) return;

    const progressBar = e.currentTarget as HTMLElement;
    const rect = progressBar.getBoundingClientRect();
    const clickPosition = (e.clientX - rect.left) / rect.width;
    const newTime = clickPosition * duration.value;

    seek(newTime);
  };

  // 调整音量
  const changeVolume = (val: number) => {
    volumeLevel.value = val;
    muted.value = val === 0;
  };

  // 静音切换
  const toggleMute = () => {
    muted.value = !muted.value;
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

  // 在鼠标移动时显示控制条
  const handleMouseMove = () => {
    showControls();
    hideControlsDelayed();
  };
  useEventListener(videoContainer, 'mousemove', handleMouseMove);

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
  const { escape, arrowLeft, arrowRight } = useMagicKeys();

  // 监听Esc键退出全屏
  watch(escape, (pressed) => {
    if (pressed && isFullscreen.value) {
      toggleFullscreen();
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

  // 切换分辨率
  const changeResolution = (label: string) => {
    const resolution = resolutions.value.find((res) => res.label === label);
    if (resolution && hls) {
      hls.currentLevel = resolution.level!;
      currentResolution.value = resolution;
      currentResolutionLabel.value = resolution.label;
    }
  };

  // 切换播放速度
  const changePlaybackSpeed = (speed: number) => {
    if (videoRef.value) {
      rate.value = speed;
      videoRef.value.playbackRate = speed;
    }
  };
</script>

<style scoped></style>
