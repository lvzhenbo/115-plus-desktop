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
      <div v-if="downloadStats.activeCount > 0" class="ml-4 text-sm text-gray-500">
        下载中 {{ downloadStats.activeCount }} 个 ·
        {{ formatSpeed(downloadStats.totalSpeed) }}
      </div>
      <div v-if="queueStatus.queueLength > 0" class="ml-2 text-sm text-gray-400">
        队列等待 {{ queueStatus.queueLength }} 个
      </div>
      <div v-if="queueStatus.isResuming" class="ml-2 text-sm text-blue-400"> 正在恢复下载...</div>
    </NSpace>
    <NDataTable
      ref="tableRef"
      remote
      flex-height
      :columns
      :data="displayList"
      :row-key="(row: DownLoadFile) => row.gid"
      class="h-[calc(100vh-133px)]"
    />
  </div>
</template>

<script setup lang="tsx">
  import { pause, unpause, pauseAll, unpauseAll } from '@/api/aria2';
  import type { DownLoadFile } from '@/store/setting';
  import { useDownloadManager } from '@/composables/useDownloadManager';
  import {
    DeleteOutlined,
    FolderOutlined,
    ClearOutlined,
    PauseCircleOutlined,
    PlayCircleOutlined,
    ReloadOutlined,
  } from '@vicons/antd';
  import { filesize } from 'filesize';
  import type { DataTableColumns } from 'naive-ui';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';

  const {
    displayList,
    retryDownload,
    removeTask,
    clearFinished,
    pauseFolder,
    resumeFolder,
    downloadStats,
    queueStatus,
  } = useDownloadManager();
  const message = useMessage();
  const dialog = useDialog();

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

  const columns: DataTableColumns<DownLoadFile> = [
    {
      title: '文件名',
      key: 'name',
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
            <span class="truncate">{row.name}</span>
          </div>
        );
      },
    },
    {
      title: '大小',
      key: 'size',
      width: 100,
      render(row) {
        return row.size ? filesize(row.size, { standard: 'jedec' }) : '';
      },
    },
    {
      title: '速度',
      key: 'downloadSpeed',
      width: 140,
      render(row) {
        if (row.status === 'active') {
          return (
            <div>
              <div>{formatSpeed(row.downloadSpeed || 0)}</div>
              {row.eta ? <div class="text-xs text-gray-400">剩余 {formatEta(row.eta)}</div> : null}
            </div>
          );
        }
        return '';
      },
    },
    {
      title: '进度',
      key: 'percentDone',
      width: 300,
      render(row) {
        if (row.isFolder && row.isCollecting) {
          return <NText type="info">正在收集文件列表...</NText>;
        }

        const fileCountInfo = row.isFolder ? (
          <div class="text-xs text-gray-400">
            {row.completedFiles || 0}/{row.totalFiles || 0} 个文件
            {row.failedFiles ? `（${row.failedFiles} 个失败）` : ''}
          </div>
        ) : null;

        if (row.status === 'error') {
          return (
            <div>
              <NTooltip>
                {{
                  trigger: () => <NText type="error">下载失败</NText>,
                  default: () => row.errorMessage || '未知错误',
                }}
              </NTooltip>
              {fileCountInfo}
            </div>
          );
        } else if (row.status === 'waiting') {
          return <NText type="warning">等待中</NText>;
        } else if (row.status === 'active') {
          return (
            <div>
              <NProgress type="line" percentage={Math.floor(row.progress || 0)} processing />
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
              <NText type="success">下载完成</NText>
              {fileCountInfo}
            </div>
          );
        }
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
              if (row.status === 'active') {
                return (
                  <NButton
                    text
                    type="warning"
                    onClick={async () => {
                      try {
                        if (row.isFolder) {
                          await pauseFolder(row);
                        } else {
                          await pause(row.gid);
                        }
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
                        if (row.isFolder) {
                          await resumeFolder(row);
                        } else {
                          await unpause(row.gid);
                        }
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
                        await retryDownload(row);
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
                  if (row.path) await revealItemInDir(row.path);
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
                default: () => '打开',
              }}
            </NButton>
            <NButton
              text
              type="error"
              onClick={() => {
                dialog.warning({
                  title: '是否确认删除该下载任务？',
                  content: '只会删除下载任务，不会删除文件。',
                  positiveText: '确定',
                  negativeText: '取消',
                  onPositiveClick: async () => {
                    try {
                      await removeTask(row);
                      message.success('下载任务已删除');
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
      await pauseAll();
      message.success('已暂停所有下载');
    } catch (e) {
      console.error(e);
    }
  };

  const handleResumeAll = async () => {
    try {
      await unpauseAll();
      message.success('已恢复所有下载');
    } catch (e) {
      console.error(e);
    }
  };
</script>

<style scoped></style>
