<template>
  <div class="p-4">
    <NSpace class="mb-4" align="center">
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
        上传中 {{ uploadStats.activeCount }} 个
      </div>
      <div v-if="queueStatus.queueLength > 0" class="ml-2 text-sm text-gray-400">
        队列等待 {{ queueStatus.queueLength }} 个
      </div>
    </NSpace>
    <NDataTable
      ref="tableRef"
      remote
      flex-height
      :columns
      :data="displayList"
      :row-key="(row: UploadFile) => row.id"
      class="h-[calc(100vh-141px)]"
    />
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
  import type { DataTableColumns } from 'naive-ui';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';

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

  // 列定义只关心展示与交互，真实状态切换全部委托给 useUploadManager。
  const columns: DataTableColumns<UploadFile> = [
    {
      title: '文件名',
      key: 'fileName',
      ellipsis: {
        tooltip: {
          width: 'trigger',
        },
      },
      render(row) {
        return (
          <div class="flex items-center gap-1">
            {row.isFolder ? (
              <NIcon size={16} class="shrink-0">
                <FolderOutlined />
              </NIcon>
            ) : null}
            <div class="min-w-0">
              <div class="truncate">{row.fileName}</div>
              {row.isFolder && row.totalFiles ? (
                <div class="text-xs text-gray-400">
                  {row.completedFiles || 0}/{row.totalFiles} 个文件
                  {row.failedFiles ? `（${row.failedFiles} 个失败）` : ''}
                </div>
              ) : null}
            </div>
          </div>
        );
      },
    },
    {
      title: '大小',
      key: 'fileSize',
      width: 100,
      render(row) {
        return row.fileSize ? filesize(row.fileSize, { standard: 'jedec' }) : '';
      },
    },
    {
      title: '状态',
      key: 'status',
      width: 100,
      render(row) {
        switch (row.status) {
          case 'uploading':
            return (
              <NTag size="small" type="info" bordered={false}>
                上传中
              </NTag>
            );
          case 'hashing':
            return (
              <NTag size="small" type="info" bordered={false}>
                计算哈希
              </NTag>
            );
          case 'pausing':
            return (
              <NTag size="small" type="warning" bordered={false}>
                暂停中
              </NTag>
            );
          case 'paused':
            return (
              <NTag size="small" type="warning" bordered={false}>
                已暂停
              </NTag>
            );
          case 'pending':
            return (
              <NTag size="small" type="warning" bordered={false}>
                等待中
              </NTag>
            );
          case 'complete':
            return (
              <NTag size="small" type="success" bordered={false}>
                已完成
              </NTag>
            );
          case 'error':
            return (
              <NTooltip>
                {{
                  trigger: () => (
                    <NTag size="small" type="error" bordered={false}>
                      上传失败
                    </NTag>
                  ),
                  default: () => row.errorMessage || '未知错误',
                }}
              </NTooltip>
            );
          case 'cancelled':
            return (
              <NTag size="small" type="warning" bordered={false}>
                已取消
              </NTag>
            );
          default:
            return null;
        }
      },
    },
    {
      title: '进度',
      key: 'progress',
      width: 200,
      render(row) {
        if (row.status === 'uploading' || row.status === 'hashing')
          return (
            <div>
              <NProgress type="line" percentage={Math.floor(row.progress || 0)} processing />
              {row.status === 'hashing' ? (
                <div class="text-xs text-gray-400">正在计算文件哈希...</div>
              ) : null}
            </div>
          );
        if (row.status === 'pausing' || row.status === 'paused')
          return (
            <NProgress type="line" percentage={Math.floor(row.progress || 0)} status="warning" />
          );
        return '';
      },
    },
    {
      title: '操作',
      key: 'action',
      width: 280,
      render: (row) => {
        return (
          <NSpace>
            {(() => {
              if (
                row.status === 'uploading' ||
                row.status === 'hashing' ||
                row.status === 'pending' ||
                row.status === 'pausing'
              ) {
                return (
                  <NButton
                    text
                    type="warning"
                    disabled={isBatchOperating.value || row.status === 'pausing'}
                    onClick={async () => {
                      try {
                        await pauseTask(row);
                      } catch (e) {
                        console.error(e);
                        message.error(getActionErrorMessage(e, '暂停上传失败'));
                      }
                    }}
                  >
                    {{
                      icon: () => (
                        <NIcon>
                          <PauseCircleOutlined />
                        </NIcon>
                      ),
                      default: () => (row.status === 'pausing' ? '暂停中' : '暂停'),
                    }}
                  </NButton>
                );
              } else if (row.status === 'paused') {
                return (
                  <NButton
                    text
                    type="primary"
                    disabled={isBatchOperating.value}
                    onClick={async () => {
                      try {
                        await resumeTask(row);
                        message.success('已恢复上传');
                      } catch (e) {
                        console.error(e);
                        message.error(getActionErrorMessage(e, '恢复上传失败'));
                      }
                    }}
                  >
                    {{
                      icon: () => (
                        <NIcon>
                          <PlayCircleOutlined />
                        </NIcon>
                      ),
                      default: () => '继续',
                    }}
                  </NButton>
                );
              } else if (row.status === 'error') {
                return (
                  <NButton
                    text
                    type="info"
                    disabled={isBatchOperating.value}
                    onClick={async () => {
                      try {
                        await retryTask(row);
                        message.success('重试任务已添加');
                      } catch (e) {
                        console.error(e);
                        message.error(getActionErrorMessage(e, '重试失败'));
                      }
                    }}
                  >
                    {{
                      icon: () => (
                        <NIcon>
                          <ReloadOutlined />
                        </NIcon>
                      ),
                      default: () => '重试',
                    }}
                  </NButton>
                );
              } else {
                return null;
              }
            })()}
            <NButton
              text
              onClick={async () => {
                try {
                  if (row.filePath) await revealItemInDir(row.filePath);
                } catch (e) {
                  console.error(e);
                  message.error('打开文件失败，请检查文件是否存在');
                }
              }}
            >
              {{
                icon: () => (
                  <NIcon>
                    <FolderOutlined />
                  </NIcon>
                ),
                default: () => '打开本地',
              }}
            </NButton>
            {row.status === 'complete' ? (
              <NButton
                text
                type="primary"
                onClick={() => {
                  router.push({ path: '/home', query: { fid: row.targetCid } });
                }}
              >
                {{
                  icon: () => (
                    <NIcon>
                      <CloudOutlined />
                    </NIcon>
                  ),
                  default: () => '打开远程',
                }}
              </NButton>
            ) : null}
            <NButton
              text
              type="error"
              disabled={isBatchOperating.value}
              onClick={() => {
                dialog.warning({
                  title: '是否确认删除该上传任务？',
                  content: '只会删除上传任务记录，不会删除本地文件和已上传的远程文件。',
                  positiveText: '确定',
                  negativeText: '取消',
                  onPositiveClick: async () => {
                    try {
                      await removeTask(row);
                      message.success('上传任务已删除');
                    } catch (e) {
                      console.error(e);
                      message.error(getActionErrorMessage(e, '删除上传任务失败'));
                    }
                  },
                });
              }}
            >
              {{
                icon: () => (
                  <NIcon>
                    <DeleteOutlined />
                  </NIcon>
                ),
                default: () => '删除',
              }}
            </NButton>
          </NSpace>
        );
      },
    },
  ];

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
