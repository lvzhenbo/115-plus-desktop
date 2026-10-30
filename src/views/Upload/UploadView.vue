<template>
  <NEl class="flex flex-col h-[calc(100vh-59px)] text-(--text-color-2)">
    <ExplorerToolbarLayout>
      <NTooltip>
        <template #trigger>
          <NButton size="small" quaternary :disabled="isBatchOperating" @click="handleClear">
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
          <NButton
            size="small"
            quaternary
            :disabled="isBatchOperating || !hasPausableTasks"
            :loading="isPausingAll"
            @click="handlePauseAll"
          >
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
          <NButton
            size="small"
            quaternary
            :disabled="isBatchOperating || !hasResumableTasks"
            :loading="isResumingAll"
            @click="handleResumeAll"
          >
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
      <div v-if="uploadStats.activeCount > 0" class="text-sm text-(--text-color-2)">
        上传中 {{ uploadStats.activeCount }} 个 ·
        {{ formatSpeed(uploadStats.totalSpeed) }}
      </div>
      <div v-if="queueStatus.queueLength > 0" class="text-sm text-(--text-color-3)">
        队列等待 {{ queueStatus.queueLength }} 个
      </div>
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
              {{ formatUploadedSize(item.source) }} /
              {{ filesize(item.source.fileSize, { standard: 'jedec' }) }}
              <div v-if="item.source.status === 'error'">{{
                item.source.errorMessage || '未知错误'
              }}</div>
              <template v-if="item.source.isFolder && item.source.totalFiles">
                · {{ item.source.completedFiles || 0 }}/{{ item.source.totalFiles }} 个文件
                <span v-if="item.source.failedFiles">（{{ item.source.failedFiles }} 个失败）</span>
              </template>
            </NTooltip>
            <NText
              v-if="
                item.detail && !['pending', 'hashing', 'cancelled'].includes(item.source.status)
              "
              :type="detailTextType(item.source)"
              depth="2"
              class="min-w-0 truncate text-xs leading-5"
              :title="item.detail"
              >{{ item.detail }}</NText
            >
          </div>
          <div class="w-40 shrink-0 px-2 flex items-center gap-1 self-stretch" @click.stop>
            <NTooltip
              v-if="
                item.source.status === 'uploading' ||
                item.source.status === 'hashing' ||
                item.source.status === 'pending' ||
                item.source.status === 'pausing'
              "
            >
              <template #trigger>
                <NButton
                  quaternary
                  size="small"
                  type="warning"
                  :disabled="isBatchOperating || item.source.status === 'pausing'"
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
                  :disabled="isBatchOperating"
                  @click="handleResumeItem(item.source)"
                >
                  <template #icon
                    ><NIcon size="14"><PlayCircleOutlined /></NIcon
                  ></template>
                </NButton>
              </template>
              继续
            </NTooltip>
            <NTooltip v-else-if="item.source.status === 'error'">
              <template #trigger>
                <NButton
                  quaternary
                  size="small"
                  type="info"
                  :disabled="isBatchOperating"
                  @click="handleRetry(item.source)"
                >
                  <template #icon
                    ><NIcon size="14"><ReloadOutlined /></NIcon
                  ></template>
                </NButton>
              </template>
              重试
            </NTooltip>
            <NTooltip>
              <template #trigger>
                <NButton size="small" quaternary @click="handleOpenLocal(item.source)">
                  <template #icon
                    ><NIcon size="14"><FolderOutlined /></NIcon
                  ></template>
                </NButton>
              </template>
              打开本地
            </NTooltip>
            <NTooltip v-if="item.source.status === 'complete'">
              <template #trigger>
                <NButton
                  quaternary
                  size="small"
                  type="primary"
                  @click="handleOpenRemote(item.source)"
                >
                  <template #icon
                    ><NIcon size="14"><CloudOutlined /></NIcon
                  ></template>
                </NButton>
              </template>
              打开远程
            </NTooltip>
            <NTooltip>
              <template #trigger>
                <NButton
                  quaternary
                  size="small"
                  type="error"
                  :disabled="isBatchOperating"
                  @click="handleDeleteItem(item.source)"
                >
                  <template #icon
                    ><NIcon size="14"><DeleteOutlined /></NIcon
                  ></template>
                </NButton>
              </template>
              删除任务
            </NTooltip>
          </div>
        </template>
      </FileItemView>
    </NScrollbar>
    <div v-else class="flex-1 flex items-center justify-center"
      ><NEmpty description="暂无上传任务"
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
  import { useUploadManager } from '@/composables/useUploadManager';
  import type { UploadFile } from '@/composables/useUploadManager';
  import {
    DeleteOutlined,
    FolderOutlined,
    ClearOutlined,
    PauseCircleOutlined,
    PlayCircleOutlined,
    ReloadOutlined,
    CloudOutlined,
  } from '@vicons/antd';
  import { filesize } from 'filesize';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';

  const formatSpeed = (speed: number) => {
    if (!speed) return '0 B/s';
    return filesize(speed, { standard: 'jedec' }) + '/s';
  };

  const formatEta = (seconds?: number) => {
    if (!seconds || seconds <= 0) return '';
    if (seconds < 60) return `${seconds}秒`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}分${seconds % 60}秒`;
    const hours = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    return `${hours}时${mins}分`;
  };

  const {
    displayList,
    pauseTask,
    pauseAllTasks,
    resumeTask,
    resumeAllTasks,
    retryTask,
    removeTask,
    clearFinished,
    isBatchOperating,
    isPausingAll,
    isResumingAll,
    queueStatus,
    uploadStats,
  } = useUploadManager();
  const message = useMessage();
  const dialog = useDialog();
  const selectedItems = new Set<string>();
  const items = computed(() =>
    displayList.value.map((source) => ({
      fid: source.id,
      fn: source.fileName,
      fc: source.isFolder ? '0' : '1',
      ico: source.fileName.split('.').pop()?.toLowerCase() || '',
      fs: source.fileSize,
      taskStatus: getTaskStatus(source),
      detail: [detailLeft(source), detailRight(source)].filter(Boolean).join(' · '),
      source,
    })),
  );

  const router = useRouter();

  const hasPausableTasks = computed(() =>
    displayList.value.some(
      (item) =>
        item.status === 'pending' || item.status === 'hashing' || item.status === 'uploading',
    ),
  );
  const hasResumableTasks = computed(() =>
    displayList.value.some((item) => item.status === 'paused'),
  );

  const getActionErrorMessage = (error: unknown, fallback: string) => {
    if (error instanceof Error && error.message) {
      return error.message;
    }

    return fallback;
  };

  /** 根据进度百分比计算已上传字节数并格式化 */
  const formatUploadedSize = (item: UploadFile) => {
    const uploaded = Math.round(((item.progress || 0) / 100) * (item.fileSize || 0));
    return filesize(uploaded, { standard: 'jedec' });
  };

  /** 已完成固定 100%，其余取实际进度。 */
  const progressValue = (item: UploadFile) => {
    if (item.status === 'complete') return 100;
    return Math.max(0, Math.min(100, Math.round(item.progress || 0)));
  };

  function getTaskStatus(item: UploadFile): { type: TaskStatusType; text: string } {
    switch (item.status) {
      case 'complete':
        return { type: 'success', text: '已完成' };
      case 'error':
        return { type: 'failure', text: '上传失败' };
      case 'hashing':
        return { type: 'analysis', text: '计算哈希' };
      case 'uploading':
        return { type: 'downloading', text: `${progressValue(item)}%` };
      case 'pausing':
        return { type: 'downloading', text: '暂停中' };
      case 'paused':
        return { type: 'downloading', text: '已暂停' };
      case 'cancelled':
        return { type: 'downloading', text: '已取消' };
      default:
        return { type: 'downloading', text: '等待中' };
    }
  }

  /** 详情行左侧文字类型：错误状态用 error 色，其余用默认 */
  const detailTextType = (item: UploadFile): 'error' | undefined => {
    if (item.status === 'error') {
      return 'error';
    }
    return undefined;
  };

  /** 详情行左侧：速度或状态摘要 */
  const detailLeft = (item: UploadFile) => {
    switch (item.status) {
      case 'uploading':
        return `↑ ${formatSpeed(item.uploadSpeed || 0)}`;
      case 'hashing':
        return '正在计算文件哈希...';
      case 'pausing':
      case 'paused':
        return `${progressValue(item)}%`;
      case 'complete':
        return '';
      case 'error':
        return item.errorMessage || '';
      case 'pending':
        return '排队等待';
      case 'cancelled':
        return '已取消';
      default:
        return '';
    }
  };

  /** 详情行右侧：剩余时间 */
  const detailRight = (item: UploadFile) => {
    if (item.status === 'uploading' && item.etaSecs) {
      return `剩余 ${formatEta(item.etaSecs)}`;
    }
    return '';
  };

  const handlePauseItem = async (item: UploadFile) => {
    try {
      await pauseTask(item);
    } catch (e) {
      console.error(e);
      message.error(getActionErrorMessage(e, '暂停上传失败'));
    }
  };

  const handleResumeItem = async (item: UploadFile) => {
    try {
      await resumeTask(item);
      message.success('已恢复上传');
    } catch (e) {
      console.error(e);
      message.error(getActionErrorMessage(e, '恢复上传失败'));
    }
  };

  const handleRetry = async (item: UploadFile) => {
    try {
      await retryTask(item);
      message.success('重试任务已添加');
    } catch (e) {
      console.error(e);
      message.error(getActionErrorMessage(e, '重试失败'));
    }
  };

  const handleOpenLocal = async (item: UploadFile) => {
    try {
      if (item.filePath) await revealItemInDir(item.filePath);
    } catch (e) {
      console.error(e);
      message.error('打开文件失败，请检查文件是否存在');
    }
  };

  const handleOpenRemote = (item: UploadFile) => {
    router.push({ path: '/home', query: { fid: item.targetCid } });
  };

  const handleDeleteItem = (item: UploadFile) => {
    dialog.warning({
      title: '是否确认删除该上传任务？',
      content: '只会删除上传任务记录，不会删除本地文件和已上传的远程文件。',
      positiveText: '确定',
      negativeText: '取消',
      onPositiveClick: async () => {
        try {
          await removeTask(item);
          message.success('上传任务已删除');
        } catch (e) {
          console.error(e);
          message.error(getActionErrorMessage(e, '删除上传任务失败'));
        }
      },
    });
  };

  // 清理已完成/失败任务会交给后端统一删除，页面这里只负责确认交互。
  const handleClear = () => {
    dialog.warning({
      title: '是否确认清除已完成的上传任务？',
      content: '包括已完成和已失败的上传任务',
      positiveText: '确定',
      negativeText: '取消',
      onPositiveClick: async () => {
        try {
          await clearFinished();
          message.success('上传任务已清除');
        } catch (e) {
          console.error(e);
          message.error(getActionErrorMessage(e, '清除上传任务失败'));
        }
      },
    });
  };

  // 全部暂停直接走后端确认式 pause-all，确保文件上传和文件夹收集都真正停下来后再提示成功。
  const handlePauseAll = async () => {
    try {
      await pauseAllTasks();
      message.success('已暂停所有上传');
    } catch (e) {
      console.error(e);
      message.error(getActionErrorMessage(e, '暂停全部上传失败'));
    }
  };

  // 全部继续直接走后端统一恢复入口，和全部暂停一样由后端控制实际完成时机。
  const handleResumeAll = async () => {
    try {
      await resumeAllTasks();
      message.success('已恢复所有上传');
    } catch (e) {
      console.error(e);
      message.error(getActionErrorMessage(e, '恢复全部上传失败'));
    }
  };
</script>

<style scoped></style>
