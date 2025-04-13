<template>
  <div class="w-full h-full">
    <div class="bg-[#1a1a1a] h-screen w-screen overflow-hidden">
      <div
        ref="videoContainer"
        class="relative w-full h-full bg-black overflow-hidden flex justify-center items-center"
      >
        <video
          ref="videoRef"
          class="w-full h-full object-contain cursor-pointer"
          @click="togglePlay"
          @play="playing = true"
          @pause="playing = false"
          @timeupdate="updateProgress"
          @loadedmetadata="onVideoLoaded"
          @waiting="handleWaiting"
          @seeking="handleSeeking"
          @seeked="handleSeeked"
          @canplay="handleCanPlay"
        ></video>

        <!-- 视频加载中 -->
        <div
          v-if="loading || seeking"
          class="absolute inset-0 flex flex-col justify-center items-center gap-4 bg-black/70 text-white z-10"
        >
          <NSpin size="large" />
          <span>{{ seeking ? '正在定位...' : '加载中...' }}</span>
        </div>

        <!-- 视频控制条 -->
        <div
          v-show="controlsVisible"
          class="absolute bottom-0 left-0 w-full px-4 py-2 bg-gradient-to-t from-black/80 to-transparent transition-opacity duration-300 z-20"
          @mouseenter="showControls"
          @mouseleave="hideControlsDelayed"
        >
          <div class="flex items-center mb-2">
            <div class="flex-1 h-1.5 bg-white/30 rounded cursor-pointer relative" @click="seek">
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

          <div class="flex items-center gap-2 flex-wrap md:flex-nowrap">
            <NButton quaternary circle @click="togglePlay">
              <template #icon>
                <NIcon size="24" class="text-white">
                  <PauseCircleOutlined v-if="playing" />
                  <PlayCircleOutlined v-else />
                </NIcon>
              </template>
            </NButton>

            <NButton quaternary circle @click="skipBackward">
              <template #icon>
                <NIcon size="24" class="text-white"><StepBackwardOutlined /></NIcon>
              </template>
            </NButton>

            <NButton quaternary circle @click="skipForward">
              <template #icon>
                <NIcon size="24" class="text-white"><StepForwardOutlined /></NIcon>
              </template>
            </NButton>

            <div class="flex items-center w-auto md:w-40">
              <NButton quaternary circle @click="toggleMute">
                <template #icon>
                  <NIcon size="24" class="text-white">
                    <SoundOutlined v-if="!isMuted" />
                    <SoundOutlined v-else />
                  </NIcon>
                </template>
              </NButton>
              <NSlider
                v-model:value="volumeLevel"
                class="w-15 md:w-[100px] ml-2"
                :min="0"
                :max="100"
                @update:value="changeVolume"
              />
            </div>

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
            <NPopover
              trigger="click"
              placement="top"
              :show="showSpeedMenu"
              @update:show="showSpeedMenu = $event"
            >
              <template #trigger>
                <NButton quaternary circle @click="showSpeedMenu = true">
                  <template #icon>
                    <NIcon size="24" class="text-white"><DashboardOutlined /></NIcon>
                  </template>
                </NButton>
              </template>
              <div class="p-2 min-w-[140px]">
                <div class="mb-2 text-sm font-bold">选择播放速度</div>
                <NRadioGroup
                  v-model:value="currentPlaybackSpeed"
                  @update:value="changePlaybackSpeed"
                >
                  <div class="space-y-2">
                    <NRadio v-for="speed in playbackSpeeds" :key="speed" :value="speed">
                      {{ speed }}x
                    </NRadio>
                  </div>
                </NRadioGroup>
              </div>
            </NPopover>

            <NButton quaternary circle class="hidden md:flex" @click="toggleFullscreen">
              <template #icon>
                <NIcon size="24" class="text-white">
                  <FullscreenExitOutlined v-if="isFullscreen" />
                  <FullscreenOutlined v-else />
                </NIcon>
              </template>
            </NButton>

            <NInput
              v-model:value="videoUrl"
              class="w-full md:w-[360px] md:ml-auto mt-2 md:mt-0"
              type="text"
              placeholder="输入m3u8视频URL"
            >
              <template #suffix>
                <NButton quaternary @click="loadVideo">
                  <template #icon><CaretRightOutlined /></template>
                  播放
                </NButton>
              </template>
            </NInput>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
  import { ref, onMounted, onBeforeUnmount } from 'vue';
  import Hls from 'hls.js';
  import { Window } from '@tauri-apps/api/window';
  import {
    PlayCircleOutlined,
    PauseCircleOutlined,
    StepBackwardOutlined,
    StepForwardOutlined,
    SoundOutlined,
    FullscreenOutlined,
    FullscreenExitOutlined,
    CaretRightOutlined,
    SettingOutlined,
    DashboardOutlined,
  } from '@vicons/antd';
  import { useMessage } from 'naive-ui';

  interface Resolution {
    height: number;
    width?: number;
    bitrate?: number;
    url: string;
    level?: number;
    label: string;
  }

  const message = useMessage();
  const videoRef = ref<HTMLVideoElement | null>(null);
  const videoContainer = ref<HTMLElement | null>(null);
  const videoUrl = ref<string>(''); // m3u8视频URL
  const playing = ref<boolean>(false);
  const currentTime = ref<number>(0);
  const duration = ref<number>(0);
  const progress = ref<number>(0);
  const volumeLevel = ref<number>(100);
  const isMuted = ref<boolean>(false);
  const loading = ref<boolean>(false);
  const seeking = ref<boolean>(false); // 是否正在seek中（调整进度）
  const controlsVisible = ref<boolean>(true);
  const controlsTimeout = ref<number | null>(null);
  const isFullscreen = ref<boolean>(false);
  const resolutions = ref<Resolution[]>([]); // 可用的分辨率列表
  const currentResolution = ref<Resolution | null>(null); // 当前选择的分辨率
  const currentResolutionLabel = ref<string>(''); // 当前选择的分辨率标签
  const showResolutionMenu = ref<boolean>(false); // 是否显示分辨率菜单
  const playbackSpeeds = [0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4, 5]; // 播放速度选项
  const currentPlaybackSpeed = ref<number>(1); // 当前播放速度，默认1倍速
  const showSpeedMenu = ref<boolean>(false); // 是否显示播放速度菜单
  let hls: Hls | null = null;
  let checkFullscreenInterval: number | null = null; // 添加全屏状态检测的定时器引用

  // 计算视频进度百分比
  const updateProgress = () => {
    if (videoRef.value) {
      currentTime.value = videoRef.value.currentTime;
      progress.value = (currentTime.value / duration.value) * 100 || 0;
    }
  };

  // 格式化时间为 MM:SS 格式
  const formatTime = (time: number): string => {
    const minutes = Math.floor(time / 60);
    const seconds = Math.floor(time % 60);
    return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
  };

  // 加载视频
  const loadVideo = () => {
    if (!videoUrl.value) {
      message.warning('请输入有效的视频URL');
      return;
    }

    if (!videoRef.value) return;

    loading.value = true;

    // 清理现有HLS实例
    if (hls) {
      hls.destroy();
      hls = null;
    }

    // 使用HLS.js加载m3u8视频
    if (Hls.isSupported()) {
      hls = new Hls();
      hls.loadSource(videoUrl.value);
      hls.attachMedia(videoRef.value);

      hls.on(Hls.Events.MANIFEST_PARSED, (_event, _data) => {
        loading.value = false;

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

        if (videoRef.value) {
          videoRef.value.play().catch(() => {
            message.warning('自动播放失败，请点击播放按钮手动播放');
          });
        }
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
              loading.value = false;
              break;
          }
        }
      });
    } else if (videoRef.value.canPlayType('application/vnd.apple.mpegurl')) {
      // 对于原生支持HLS的浏览器（如Safari）
      videoRef.value.src = videoUrl.value;
      videoRef.value.addEventListener('loadedmetadata', () => {
        loading.value = false;
        videoRef.value?.play().catch(() => {
          message.warning('自动播放失败，请点击播放按钮手动播放');
        });
      });
    } else {
      message.error('您的浏览器不支持HLS视频播放');
      loading.value = false;
    }
  };

  // 视频加载完成
  const onVideoLoaded = () => {
    if (videoRef.value) {
      duration.value = videoRef.value.duration;
    }
  };

  // 播放/暂停切换
  const togglePlay = () => {
    if (!videoRef.value) return;

    if (videoRef.value.paused) {
      videoRef.value.play();
    } else {
      videoRef.value.pause();
    }
  };

  // 调整进度条位置
  const seek = (e: MouseEvent) => {
    if (!videoRef.value) return;

    const progressBar = e.currentTarget as HTMLElement;
    const rect = progressBar.getBoundingClientRect();
    const clickPosition = (e.clientX - rect.left) / rect.width;
    const newTime = clickPosition * duration.value;

    videoRef.value.currentTime = newTime;
  };

  // 调整音量
  const changeVolume = (val: number) => {
    if (!videoRef.value) return;

    volumeLevel.value = val;
    videoRef.value.volume = val / 100;
    isMuted.value = val === 0;
  };

  // 静音切换
  const toggleMute = () => {
    if (!videoRef.value) return;

    if (isMuted.value) {
      isMuted.value = false;
      videoRef.value.volume = volumeLevel.value / 100;
    } else {
      isMuted.value = true;
      videoRef.value.volume = 0;
    }
  };

  // 前进10秒
  const skipForward = () => {
    if (!videoRef.value) return;
    videoRef.value.currentTime += 10;
  };

  // 后退10秒
  const skipBackward = () => {
    if (!videoRef.value) return;
    videoRef.value.currentTime -= 10;
  };

  // 显示控制条
  const showControls = () => {
    controlsVisible.value = true;

    if (controlsTimeout.value) {
      clearTimeout(controlsTimeout.value);
      controlsTimeout.value = null;
    }
  };

  // 延迟隐藏控制条
  const hideControlsDelayed = () => {
    if (controlsTimeout.value) {
      clearTimeout(controlsTimeout.value);
    }

    controlsTimeout.value = window.setTimeout(() => {
      controlsVisible.value = false;
    }, 3000);
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

  // 监听全屏变更事件
  const handleFullscreenChange = () => {
    isFullscreen.value = !!document.fullscreenElement;
  };

  // 视频等待事件
  const handleWaiting = () => {
    loading.value = true;
  };

  // 视频搜寻事件
  const handleSeeking = () => {
    seeking.value = true;
  };

  // 视频搜寻完成事件
  const handleSeeked = () => {
    seeking.value = false;
  };

  // 视频可以播放事件
  const handleCanPlay = () => {
    loading.value = false;
  };

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
      videoRef.value.playbackRate = speed;
      currentPlaybackSpeed.value = speed;
    }
  };

  onMounted(async () => {
    if (videoRef.value) {
      // 设置初始音量
      videoRef.value.volume = volumeLevel.value / 100;
      // 设置初始播放速度
      videoRef.value.playbackRate = currentPlaybackSpeed.value;
    }

    // 添加鼠标移动事件监听器
    if (videoContainer.value) {
      videoContainer.value.addEventListener('mousemove', handleMouseMove);
    }

    // 添加全屏事件监听器
    document.addEventListener('fullscreenchange', handleFullscreenChange);

    // 初始化和监听 Tauri 窗口全屏状态
    try {
      // 获取当前应用的窗口实例
      const currentWindow = Window.getCurrent();

      // 初始化时同步全屏状态
      isFullscreen.value = await currentWindow.isFullscreen();

      // 由于 Tauri 2.x 没有 onFullscreenChange 事件，
      // 我们使用一个轮询来检测全屏状态变化
      checkFullscreenInterval = setInterval(async () => {
        try {
          const fullscreenState = await currentWindow.isFullscreen();
          if (isFullscreen.value !== fullscreenState) {
            isFullscreen.value = fullscreenState;
          }
        } catch (e) {
          console.error('检查全屏状态失败:', e);
        }
      }, 1000); // 每秒检查一次
    } catch (err) {
      console.error('无法监听 Tauri 窗口全屏状态:', err);
    }
  });

  onBeforeUnmount(() => {
    // 销毁HLS实例
    if (hls) {
      hls.destroy();
    }

    // 移除事件监听器
    if (videoContainer.value) {
      videoContainer.value.removeEventListener('mousemove', handleMouseMove);
    }

    document.removeEventListener('fullscreenchange', handleFullscreenChange);

    // 清除全屏状态检测的定时器
    if (checkFullscreenInterval !== null) {
      clearInterval(checkFullscreenInterval);
      checkFullscreenInterval = null;
    }

    if (controlsTimeout.value) {
      clearTimeout(controlsTimeout.value);
    }
  });
</script>

<style scoped></style>
