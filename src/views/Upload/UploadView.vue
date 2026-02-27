<template>
  <div class="px-6 py-3">
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
      class="h-[calc(100vh-133px)]"
    />
  </div>
</template>

<script setup lang="tsx">
  import type { UploadFile } from '@/db/uploads';
  import { useUploadManager } from '@/composables/useUploadManager';
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
    resumeTask,
    retryTask,
    removeTask,
    clearFinished,
    queueStatus,
    uploadStats,
  } = useUploadManager();
  const message = useMessage();
  const dialog = useDialog();
  const router = useRouter();

  const statusTextMap: Record<
    string,
    { text: string; type: 'info' | 'warning' | 'error' | 'success' }
  > = {
    pending: { text: '等待中', type: 'warning' },
    hashing: { text: '计算哈希...', type: 'info' },
    uploading: { text: '上传中', type: 'info' },
    paused: { text: '已暂停', type: 'warning' },
    complete: { text: '上传完成', type: 'success' },
    error: { text: '上传失败', type: 'error' },
    cancelled: { text: '已取消', type: 'warning' },
  };

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
            <span class="truncate">{row.fileName}</span>
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
      title: '进度',
      key: 'progress',
      width: 300,
      render(row) {
        const fileCountInfo = row.isFolder ? (
          <div class="text-xs text-gray-400">
            {row.completedFiles || 0}/{row.totalFiles || 0} 个文件
            {row.failedFiles ? `（${row.failedFiles} 个失败）` : ''}
          </div>
        ) : null;

        const statusInfo = statusTextMap[row.status] || { text: row.status, type: 'info' as const };

        if (row.status === 'error') {
          return (
            <div>
              <NTooltip>
                {{
                  trigger: () => <NText type="error">{statusInfo.text}</NText>,
                  default: () => row.errorMessage || '未知错误',
                }}
              </NTooltip>
              {fileCountInfo}
            </div>
          );
        } else if (row.status === 'uploading' || row.status === 'hashing') {
          return (
            <div>
              <NProgress type="line" percentage={Math.floor(row.progress || 0)} processing />
              {row.status === 'hashing' ? (
                <div class="text-xs text-gray-400">正在计算文件哈希...</div>
              ) : null}
              {fileCountInfo}
            </div>
          );
        } else if (row.status === 'paused') {
          return (
            <div>
              <NProgress
                type="line"
                percentage={Math.floor(row.progress || 0)}
                status="warning"
                indicator-placement="inside"
              />
              {fileCountInfo}
            </div>
          );
        } else if (row.status === 'complete') {
          return (
            <div>
              <NText type="success">{statusInfo.text}</NText>
              {fileCountInfo}
            </div>
          );
        } else if (row.status === 'pending') {
          return <NText type="warning">{statusInfo.text}</NText>;
        } else {
          return <NText>{statusInfo.text}</NText>;
        }
      },
    },
    {
      title: '操作',
      key: 'action',
      width: 320,
      render: (row) => {
        return (
          <NSpace>
            {(() => {
              if (
                row.status === 'uploading' ||
                row.status === 'hashing' ||
                row.status === 'pending'
              ) {
                return (
                  <NButton
                    text
                    type="warning"
                    onClick={async () => {
                      try {
                        await pauseTask(row);
                      } catch (e) {
                        console.error(e);
                      }
                    }}
                  >
                    {{
                      icon: () => (
                        <NIcon>
                          <PauseCircleOutlined />
                        </NIcon>
                      ),
                      default: () => '暂停',
                    }}
                  </NButton>
                );
              } else if (row.status === 'paused') {
                return (
                  <NButton
                    text
                    type="primary"
                    onClick={async () => {
                      try {
                        await resumeTask(row);
                        message.success('已恢复上传');
                      } catch (e) {
                        console.error(e);
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
                    onClick={async () => {
                      try {
                        await retryTask(row);
                        message.success('重试任务已添加');
                      } catch (e) {
                        console.error(e);
                        message.error('重试失败');
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

  const handleClear = () => {
    dialog.warning({
      title: '是否确认清除已完成的上传任务？',
      content: '包括已完成和已失败的上传任务',
      positiveText: '确定',
      negativeText: '取消',
      onPositiveClick: () => {
        clearFinished();
        message.success('上传任务已清除');
      },
    });
  };

  const handlePauseAll = async () => {
    for (const item of displayList.value) {
      if (item.status === 'uploading' || item.status === 'hashing' || item.status === 'pending') {
        try {
          await pauseTask(item);
        } catch (e) {
          console.error(e);
        }
      }
    }
    message.success('已暂停所有上传');
  };

  const handleResumeAll = async () => {
    for (const item of displayList.value) {
      if (item.status === 'paused') {
        try {
          await resumeTask(item);
        } catch (e) {
          console.error(e);
        }
      }
    }
    message.success('已恢复所有上传');
  };
</script>

<style scoped></style>
