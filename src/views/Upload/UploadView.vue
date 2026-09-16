<template>
  <div class="flex flex-col h-[calc(100vh-59px)]">
    <NSpace class="px-4 pt-4" align="center">
      <NButton type="primary" :disabled="isBatchOperating" @click="handleClear">
        <template #icon>
          <NIcon>
            <ClearOutlined />
          </NIcon>
        </template>
        清除已完成
      </NButton>
      <NButton
        :disabled="isBatchOperating || !hasPausableTasks"
        :loading="isPausingAll"
        @click="handlePauseAll"
      >
        <template #icon>
          <NIcon>
            <PauseCircleOutlined />
          </NIcon>
        </template>
        全部暂停
      </NButton>
      <NButton
        :disabled="isBatchOperating || !hasResumableTasks"
        :loading="isResumingAll"
        @click="handleResumeAll"
      >
        <template #icon>
          <NIcon>
            <PlayCircleOutlined />
          </NIcon>
        </template>
        全部继续
      </NButton>
      <div v-if="uploadStats.activeCount > 0" class="ml-4 text-sm text-gray-500">
        上传中 {{ uploadStats.activeCount }} 个 ·
        {{ formatSpeed(uploadStats.totalSpeed) }}
      </div>
      <div v-if="queueStatus.queueLength > 0" class="ml-2 text-sm text-gray-400">
        队列等待 {{ queueStatus.queueLength }} 个
      </div>
    </NSpace>

    <!-- 卡片列表（滚动条贴边，边距由内容自添） -->
    <NScrollbar v-if="displayList.length > 0" class="flex-1 min-h-0">
      <NSpace vertical class="p-4">
        <NCard
          v-for="item in displayList"
          :key="item.id"
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
                ><span class="font-bold">{{ item.fileName }}</span></NEllipsis
              >
            </div>
          </template>
          <template #header-extra>
            <div class="flex items-center gap-2">
              <NTag v-if="item.status === 'uploading'" size="small" type="info">上传中</NTag>
              <NTag v-else-if="item.status === 'hashing'" size="small" type="info">计算哈希</NTag>
              <NTag v-else-if="item.status === 'pausing'" size="small" type="warning">暂停中</NTag>
              <NTag v-else-if="item.status === 'paused'" size="small" type="warning">已暂停</NTag>
              <NTag v-else-if="item.status === 'pending'" size="small" type="default">等待中</NTag>
              <NTag v-else-if="item.status === 'complete'" size="small" type="success">已完成</NTag>
              <NTooltip v-else-if="item.status === 'error'">
                <template #trigger>
                  <NTag size="small" type="error">上传失败</NTag>
                </template>
                {{ item.errorMessage || '未知错误' }}
              </NTooltip>
              <NTag v-else-if="item.status === 'cancelled'" size="small" type="warning"
                >已取消</NTag
              >

              <NTooltip
                v-if="
                  item.status === 'uploading' ||
                  item.status === 'hashing' ||
                  item.status === 'pending' ||
                  item.status === 'pausing'
                "
              >
                <template #trigger>
                  <NButton
                    size="tiny"
                    type="warning"
                    circle
                    :disabled="isBatchOperating || item.status === 'pausing'"
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
                  <NButton
                    size="tiny"
                    type="primary"
                    circle
                    :disabled="isBatchOperating"
                    @click="handleResumeItem(item)"
                  >
                    <template #icon
                      ><NIcon size="14"><PlayCircleOutlined /></NIcon
                    ></template>
                  </NButton>
                </template>
                继续
              </NTooltip>
              <NTooltip v-else-if="item.status === 'error'">
                <template #trigger>
                  <NButton
                    size="tiny"
                    type="info"
                    circle
                    :disabled="isBatchOperating"
                    @click="handleRetry(item)"
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
                  <NButton size="tiny" quaternary circle @click="handleOpenLocal(item)">
                    <template #icon
                      ><NIcon size="14"><FolderOutlined /></NIcon
                    ></template>
                  </NButton>
                </template>
                打开本地
              </NTooltip>
              <NTooltip v-if="item.status === 'complete'">
                <template #trigger>
                  <NButton size="tiny" type="primary" circle @click="handleOpenRemote(item)">
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
                    size="tiny"
                    type="error"
                    circle
                    :disabled="isBatchOperating"
                    @click="handleDeleteItem(item)"
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

          <div class="flex flex-col gap-1">
            <!-- 信息行 -->
            <div class="flex items-center justify-between">
              <NText depth="2">{{
                item.fileSize
                  ? `${formatUploadedSize(item)} / ${filesize(item.fileSize, { standard: 'jedec' })}`
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
              :processing="item.status === 'uploading' || item.status === 'hashing'"
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

    <!-- 空状态 -->
    <div v-else class="flex-1 flex items-center justify-center">
      <NEmpty description="暂无上传任务" />
    </div>
  </div>
</template>

<script setup lang="tsx">
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

  /** NCard 紧凑主题覆盖 */
  const cardThemeOverrides = {
    paddingSmall: '8px 12px 8px',
  };

  /** 根据进度百分比计算已上传字节数并格式化 */
  const formatUploadedSize = (item: UploadFile) => {
    const uploaded = Math.round(((item.progress || 0) / 100) * (item.fileSize || 0));
    return filesize(uploaded, { standard: 'jedec' });
  };

  /** 进度条百分比：已完成固定 100%，其余取实际值 */
  const progressValue = (item: UploadFile) => {
    if (item.status === 'complete') return 100;
    return Math.floor(item.progress || 0);
  };

  /** 进度条状态色 */
  const progressStatus = (
    item: UploadFile,
  ): 'success' | 'warning' | 'error' | 'info' | undefined => {
    switch (item.status) {
      case 'complete':
        return 'success';
      case 'pausing':
      case 'paused':
        return 'warning';
      case 'error':
        return 'error';
      default:
        return undefined;
    }
  };

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
        return `${Math.floor(item.progress || 0)}%`;
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
