<template>
  <div class="p-4">
    <!-- 工具栏 -->
    <NSpace class="mb-4" align="center">
      <NButton type="primary" @click="handleClear">
        <template #icon>
          <NIcon>
            <ClearOutlined />
          </NIcon>
        </template>
        清除已完成
      </NButton>
      <NButton @click="handlePauseAll">
        <template #icon>
          <NIcon>
            <PauseCircleOutlined />
          </NIcon>
        </template>
        全部暂停
      </NButton>
      <NButton @click="handleResumeAll">
        <template #icon>
          <NIcon>
            <PlayCircleOutlined />
          </NIcon>
        </template>
        全部继续
      </NButton>
      <NText v-if="downloadStats.activeCount > 0" depth="2" class="ml-4 text-sm">
        下载中 {{ downloadStats.activeCount }} 个 ·
        {{ formatSpeed(downloadStats.totalSpeed) }}
      </NText>
      <NText v-if="queueStatus.queueLength > 0" depth="3" class="ml-2 text-sm">
        队列等待 {{ queueStatus.queueLength }} 个
      </NText>
    </NSpace>

    <!-- 卡片列表 -->
    <NScrollbar class="h-[calc(100vh-141px)]!">
      <NEmpty v-if="displayList.length === 0" description="暂无下载任务" class="h-full" />
      <NSpace v-else vertical>
        <NCard
          v-for="item in displayList"
          :key="item.gid"
          hoverable
          size="small"
          :theme-overrides="cardThemeOverrides"
        >
          <template #header>
            <div class="min-w-0 flex items-center gap-1">
              <NIcon v-if="item.isFolder" class="shrink-0">
                <FolderOutlined />
              </NIcon>
              <NEllipsis
                ><span class="font-bold">{{ item.name }}</span></NEllipsis
              >
            </div>
          </template>
          <template #header-extra>
            <div class="flex items-center gap-2">
              <NTag v-if="item.isFolder && item.isCollecting" size="small" type="info">
                收集文件中
              </NTag>
              <NTag v-else-if="item.status === 'active'" size="small" type="info"> 下载中 </NTag>
              <NTag v-else-if="item.status === 'pausing'" size="small" type="warning">
                暂停中
              </NTag>
              <NTag v-else-if="item.status === 'paused'" size="small" type="warning"> 已暂停 </NTag>
              <NTag v-else-if="item.status === 'waiting'" size="small" type="default">
                等待中
              </NTag>
              <NTag v-else-if="item.status === 'complete'" size="small" type="success">
                已完成
              </NTag>
              <NTooltip v-else-if="item.status === 'error' || item.status === 'partial_error'">
                <template #trigger>
                  <NTag size="small" type="error">下载失败</NTag>
                </template>
                {{ item.errorMessage || '未知错误' }}
              </NTooltip>
              <NTooltip v-else-if="item.status === 'verify_failed'">
                <template #trigger>
                  <NTag size="small" type="error">校验失败</NTag>
                </template>
                {{ item.errorMessage || 'SHA1校验失败' }}
              </NTooltip>
              <template v-if="!item.isCollecting">
                <NTooltip v-if="item.status === 'active' || item.status === 'pausing'">
                  <template #trigger>
                    <NButton
                      size="tiny"
                      type="warning"
                      circle
                      :disabled="item.status === 'pausing'"
                      @click="handlePauseItem(item)"
                    >
                      <template #icon
                        ><NIcon size="14"><PauseCircleOutlined /></NIcon
                      ></template>
                    </NButton>
                  </template>
                  暂停
                </NTooltip>
                <NTooltip v-else-if="item.status === 'paused'">
                  <template #trigger>
                    <NButton size="tiny" type="primary" circle @click="handleResumeItem(item)">
                      <template #icon
                        ><NIcon size="14"><PlayCircleOutlined /></NIcon
                      ></template>
                    </NButton>
                  </template>
                  继续
                </NTooltip>
                <NTooltip
                  v-else-if="
                    item.status === 'error' ||
                    item.status === 'partial_error' ||
                    item.status === 'verify_failed'
                  "
                >
                  <template #trigger>
                    <NButton size="tiny" type="info" circle @click="handleRetry(item)">
                      <template #icon
                        ><NIcon size="14"><ReloadOutlined /></NIcon
                      ></template>
                    </NButton>
                  </template>
                  重试
                </NTooltip>
                <NTooltip>
                  <template #trigger>
                    <NButton size="tiny" quaternary circle @click="handleOpenInDir(item)">
                      <template #icon
                        ><NIcon size="14"><FolderOutlined /></NIcon
                      ></template>
                    </NButton>
                  </template>
                  打开所在位置
                </NTooltip>
                <NTooltip>
                  <template #trigger>
                    <NButton size="tiny" type="error" circle @click="handleDeleteItem(item)">
                      <template #icon
                        ><NIcon size="14"><DeleteOutlined /></NIcon
                      ></template>
                    </NButton>
                  </template>
                  删除任务
                </NTooltip>
              </template>
            </div>
          </template>

          <div class="flex flex-col gap-1">
            <!-- 信息行 -->
            <div class="flex items-center justify-between">
              <NText depth="2">{{
                item.size
                  ? `${formatDownloadedSize(item)} / ${filesize(item.size, { standard: 'jedec' })}`
                  : ''
              }}</NText>
              <span v-if="item.isFolder && item.totalFiles">
                <NText depth="3">{{ item.completedFiles || 0 }}/{{ item.totalFiles }} 个文件</NText>
                <NText v-if="item.failedFiles" type="error"
                  >（{{ item.failedFiles }} 个失败）</NText
                >
              </span>
            </div>

            <!-- 进度条：始终渲染 -->
            <NProgress
              type="line"
              :percentage="progressValue(item)"
              :status="progressStatus(item)"
              :processing="item.status === 'active'"
            />

            <!-- 详情行：始终渲染 -->
            <div class="flex items-center justify-between text-xs min-h-4">
              <NText :type="detailTextType(item)" depth="2">{{ detailLeft(item) }}</NText>
              <NText depth="3">{{ detailRight(item) }}</NText>
            </div>
          </div>
        </NCard>
      </NSpace>
    </NScrollbar>
  </div>
</template>

<script setup lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { useDownloadManager, type DownLoadFile } from '@/composables/useDownloadManager';
  import {
    DeleteOutlined,
    FolderOutlined,
    ClearOutlined,
    PauseCircleOutlined,
    PlayCircleOutlined,
    ReloadOutlined,
  } from '@vicons/antd';
  import { filesize } from 'filesize';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { intervalToDuration, formatDuration } from 'date-fns';
  import { zhCN } from 'date-fns/locale/zh-CN';

  const {
    displayList,
    retryDownload,
    removeTask,
    clearFinished,
    pauseFolder,
    resumeFolder,
    resumeSingleFile,
    pauseAllTasks,
    resumeAllTasks,
    downloadStats,
    queueStatus,
  } = useDownloadManager();
  const message = useMessage();
  const dialog = useDialog();

  const formatSpeed = (speed: number) => {
    if (!speed) return '0 B/s';
    return filesize(speed, { standard: 'jedec' }) + '/s';
  };

  /** 根据进度百分比计算已下载字节数并格式化 */
  const formatDownloadedSize = (item: DownLoadFile) => {
    const downloaded = Math.round(((item.progress || 0) / 100) * (item.size || 0));
    return filesize(downloaded, { standard: 'jedec' });
  };

  /** NCard 紧凑主题覆盖 */
  const cardThemeOverrides = {
    paddingSmall: '8px 12px 8px',
  };

  const formatEta = (seconds?: number) => {
    if (!seconds || seconds <= 0) return '';
    const duration = intervalToDuration({ start: 0, end: seconds * 1000 });
    return formatDuration(duration, { locale: zhCN });
  };

  const handleClear = () => {
    dialog.warning({
      title: '是否确认清除已完成的下载任务？',
      content: '包括已完成和已失败的下载任务',
      positiveText: '确定',
      negativeText: '取消',
      onPositiveClick: () => {
        clearFinished();
        message.success('下载任务已清除');
      },
    });
  };

  const handlePauseAll = async () => {
    try {
      await pauseAllTasks();
      message.success('已暂停所有下载');
    } catch (e) {
      console.error(e);
    }
  };

  const handleResumeAll = async () => {
    try {
      await resumeAllTasks();
      message.success('已恢复所有下载');
    } catch (e) {
      console.error(e);
    }
  };

  const handlePauseItem = async (item: DownLoadFile) => {
    try {
      if (item.isFolder) {
        await pauseFolder(item);
      } else {
        item.status = 'pausing';
        await invoke('download_pause_task', { gid: item.gid });
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleResumeItem = async (item: DownLoadFile) => {
    try {
      if (item.isFolder) {
        await resumeFolder(item);
      } else {
        await resumeSingleFile(item);
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleRetry = async (item: DownLoadFile) => {
    try {
      await retryDownload(item);
      message.success('重试任务已添加');
    } catch (e) {
      console.error(e);
      message.error('重试失败');
    }
  };

  const handleOpenInDir = async (item: DownLoadFile) => {
    try {
      if (item.path) await revealItemInDir(item.path);
    } catch (e) {
      console.error(e);
      message.error('打开文件失败，请检查文件是否存在');
    }
  };

  const handleDeleteItem = (item: DownLoadFile) => {
    dialog.warning({
      title: '是否确认删除该下载任务？',
      content: '只会删除下载任务，不会删除文件。',
      positiveText: '确定',
      negativeText: '取消',
      onPositiveClick: async () => {
        try {
          await removeTask(item);
          message.success('下载任务已删除');
        } catch (e) {
          console.error(e);
        }
      },
    });
  };

  /** 进度条百分比：已完成固定 100%，其余取实际值 */
  const progressValue = (item: DownLoadFile) => {
    if (item.status === 'complete') return 100;
    return Math.floor(item.progress || 0);
  };

  /** 进度条状态色 */
  const progressStatus = (
    item: DownLoadFile,
  ): 'success' | 'warning' | 'error' | 'info' | undefined => {
    switch (item.status) {
      case 'complete':
        return 'success';
      case 'paused':
      case 'pausing':
        return 'warning';
      case 'error':
      case 'partial_error':
      case 'verify_failed':
        return 'error';
      default:
        return undefined;
    }
  };

  /** 详情行左侧文字类型：错误状态用 error 色，其余用默认 */
  const detailTextType = (item: DownLoadFile): 'error' | undefined => {
    if (
      item.status === 'error' ||
      item.status === 'partial_error' ||
      item.status === 'verify_failed'
    ) {
      return 'error';
    }
    return undefined;
  };

  /** 详情行左侧：速度或状态摘要 */
  const detailLeft = (item: DownLoadFile) => {
    switch (item.status) {
      case 'active':
        return `↓ ${formatSpeed(item.downloadSpeed || 0)}`;
      case 'paused':
      case 'pausing':
        return `${Math.floor(item.progress || 0)}%`;
      case 'complete':
        return '';
      case 'error':
      case 'partial_error':
      case 'verify_failed':
        return item.errorMessage || '';
      case 'waiting':
        return '排队等待';
      default:
        return item.isCollecting ? '收集文件列表中...' : '';
    }
  };

  /** 详情行右侧：补充信息 */
  const detailRight = (item: DownLoadFile) => {
    if (item.status === 'active' && item.eta) {
      return `剩余 ${formatEta(item.eta)}`;
    }
    return '';
  };
</script>

<style scoped></style>
