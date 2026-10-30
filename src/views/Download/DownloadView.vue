<template>
  <NEl class="flex flex-col h-[calc(100vh-59px)] text-(--text-color-2)">
    <ExplorerToolbarLayout>
      <NTooltip>
        <template #trigger>
          <NButton size="small" quaternary @click="handleClear">
            <template #icon>
              <NIcon>
                <ClearOutlined />
              </NIcon>
            </template>
          </NButton>
        </template>
        清除已完成
      </NTooltip>
      <NTooltip>
        <template #trigger>
          <NButton size="small" quaternary @click="handlePauseAll">
            <template #icon>
              <NIcon>
                <PauseCircleOutlined />
              </NIcon>
            </template>
          </NButton>
        </template>
        全部暂停
      </NTooltip>
      <NTooltip>
        <template #trigger>
          <NButton size="small" quaternary @click="handleResumeAll">
            <template #icon>
              <NIcon>
                <PlayCircleOutlined />
              </NIcon>
            </template>
          </NButton>
        </template>
        全部继续
      </NTooltip>
      <div class="flex-1" />
      <NText v-if="downloadStats.activeCount > 0" depth="2" class="text-sm">
        下载中 {{ downloadStats.activeCount }} 个 ·
        {{ formatSpeed(downloadStats.totalSpeed) }}
      </NText>
      <NText v-if="queueStatus.queueLength > 0" depth="3" class="text-sm">
        队列等待 {{ queueStatus.queueLength }} 个
      </NText>
    </ExplorerToolbarLayout>
    <ListHeader
      v-if="items.length"
      :items
      :selected-items="selectedItems"
      :show-checkbox="false"
      :columns="['size']"
      sort-disabled
    >
      <template #after>
        <div class="w-64 shrink-0 px-2">进度</div>
        <div class="w-40 shrink-0 px-2">操作</div>
      </template>
    </ListHeader>
    <NScrollbar v-if="items.length" class="flex-1 min-h-0">
      <FileItemView
        v-for="item in items"
        :key="item.fid"
        :item
        :selected="false"
        :show-checkbox="false"
        :columns="['size']"
        view-mode="list"
      >
        <template #size="{ size }">{{ size }}</template>
        <template #after>
          <div class="w-64 shrink-0 px-2 flex items-center gap-3 self-stretch">
            <NTooltip>
              <template #trigger>
                <TaskStatus
                  class="shrink-0"
                  :type="item.taskStatus.type"
                  :text="item.taskStatus.text"
                />
              </template>
              {{ formatDownloadedSize(item.source) }} /
              {{ filesize(item.source.size, { standard: 'jedec' }) }}
              <div
                v-if="
                  item.source.status === 'error' ||
                  item.source.status === 'partial_error' ||
                  item.source.status === 'verify_failed'
                "
                >{{ item.source.errorMessage || '未知错误' }}</div
              >
              <template v-if="item.source.isFolder && item.source.totalFiles">
                · {{ item.source.completedFiles || 0 }}/{{ item.source.totalFiles }} 个文件
                <span v-if="item.source.failedFiles">（{{ item.source.failedFiles }} 个失败）</span>
              </template>
            </NTooltip>
            <NText
              v-if="item.detail && item.source.status !== 'waiting' && !item.source.isCollecting"
              :type="detailTextType(item.source)"
              depth="2"
              class="min-w-0 truncate text-xs leading-5"
              :title="item.detail"
              >{{ item.detail }}</NText
            >
          </div>
          <div class="w-40 shrink-0 px-2 flex items-center gap-1 self-stretch" @click.stop>
            <template v-if="!item.source.isCollecting">
              <NTooltip v-if="item.source.status === 'active' || item.source.status === 'pausing'">
                <template #trigger>
                  <NButton
                    quaternary
                    size="small"
                    type="warning"
                    :disabled="item.source.status === 'pausing'"
                    @click="handlePauseItem(item.source)"
                  >
                    <template #icon
                      ><NIcon size="14"><PauseCircleOutlined /></NIcon
                    ></template>
                  </NButton>
                </template>
                暂停
              </NTooltip>
              <NTooltip v-else-if="item.source.status === 'paused'">
                <template #trigger>
                  <NButton
                    quaternary
                    size="small"
                    type="primary"
                    @click="handleResumeItem(item.source)"
                  >
                    <template #icon
                      ><NIcon size="14"><PlayCircleOutlined /></NIcon
                    ></template>
                  </NButton>
                </template>
                继续
              </NTooltip>
              <NTooltip
                v-else-if="
                  item.source.status === 'error' ||
                  item.source.status === 'partial_error' ||
                  item.source.status === 'verify_failed'
                "
              >
                <template #trigger>
                  <NButton quaternary size="small" type="info" @click="handleRetry(item.source)">
                    <template #icon
                      ><NIcon size="14"><ReloadOutlined /></NIcon
                    ></template>
                  </NButton>
                </template>
                重试
              </NTooltip>
              <NTooltip>
                <template #trigger>
                  <NButton size="small" quaternary @click="handleOpenInDir(item.source)">
                    <template #icon
                      ><NIcon size="14"><FolderOutlined /></NIcon
                    ></template>
                  </NButton>
                </template>
                打开所在位置
              </NTooltip>
              <NTooltip>
                <template #trigger>
                  <NButton
                    quaternary
                    size="small"
                    type="error"
                    @click="handleDeleteItem(item.source)"
                  >
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
      </FileItemView>
    </NScrollbar>
    <div v-else class="flex-1 flex items-center justify-center"
      ><NEmpty description="暂无下载任务"
    /></div>
    <ExplorerStatusbar
      :total-count="items.length"
      :selected-count="0"
      :current-page="1"
      :total-pages="1"
      :page-size="items.length || 50"
      :show-pagination="false"
    />
  </NEl>
</template>

<script setup lang="ts">
  import TaskStatus, { type TaskStatusType } from '@/components/TaskStatus/TaskStatus.vue';
  import ExplorerToolbarLayout from '@/components/FileExplorer/components/ExplorerToolbarLayout/ExplorerToolbarLayout.vue';
  import ListHeader from '@/components/FileExplorer/components/ListHeader/ListHeader.vue';
  import FileItemView from '@/components/FileExplorer/components/FileItemView/FileItemView.vue';
  import ExplorerStatusbar from '@/components/FileExplorer/components/ExplorerStatusbar/ExplorerStatusbar.vue';
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
  const selectedItems = new Set<string>();
  const items = computed(() =>
    displayList.value.map((source) => ({
      fid: source.gid,
      fn: source.name,
      fc: source.isFolder ? '0' : '1',
      ico: source.name.split('.').pop()?.toLowerCase() || '',
      fs: source.size,
      taskStatus: getTaskStatus(source),
      detail: [detailLeft(source), detailRight(source)].filter(Boolean).join(' · '),
      source,
    })),
  );

  const formatSpeed = (speed: number) => {
    if (!speed) return '0 B/s';
    return filesize(speed, { standard: 'jedec' }) + '/s';
  };

  /** 根据进度百分比计算已下载字节数并格式化 */
  const formatDownloadedSize = (item: DownLoadFile) => {
    const downloaded = Math.round(((item.progress || 0) / 100) * (item.size || 0));
    return filesize(downloaded, { standard: 'jedec' });
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

  /** 已完成固定 100%，其余取实际进度。 */
  const progressValue = (item: DownLoadFile) => {
    if (item.status === 'complete') return 100;
    return Math.max(0, Math.min(100, Math.round(item.progress || 0)));
  };

  function getTaskStatus(item: DownLoadFile): { type: TaskStatusType; text: string } {
    if (item.isFolder && item.isCollecting) return { type: 'analysis', text: '收集文件中' };
    switch (item.status) {
      case 'complete':
        return { type: 'success', text: '已完成' };
      case 'error':
      case 'partial_error':
        return { type: 'failure', text: '下载失败' };
      case 'verify_failed':
        return { type: 'failure', text: '校验失败' };
      case 'active':
        return { type: 'downloading', text: `${progressValue(item)}%` };
      case 'pausing':
        return { type: 'downloading', text: '暂停中' };
      case 'paused':
        return { type: 'downloading', text: '已暂停' };
      default:
        return { type: 'downloading', text: '等待中' };
    }
  }

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
        return `${progressValue(item)}%`;
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
