<template>
  <div class="p-4">
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
    </NSpace>
    <NDataTable
      ref="tableRef"
      remote
      flex-height
      :columns
      :data="displayList"
      :row-key="(row: DownLoadFile) => row.gid"
      class="h-[calc(100vh-141px)]"
    />
  </div>
</template>

<script setup lang="tsx">
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
  import type { DataTableColumns } from 'naive-ui';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';

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
            <div class="min-w-0">
              <div class="truncate">{row.name}</div>
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
      key: 'size',
      width: 100,
      render(row) {
        return row.size ? filesize(row.size, { standard: 'jedec' }) : '';
      },
    },
    {
      title: '状态',
      key: 'status',
      width: 100,
      render(row) {
        if (row.isFolder && row.isCollecting)
          return (
            <NTag size="small" type="info" bordered={false}>
              收集文件中
            </NTag>
          );
        switch (row.status) {
          case 'active':
            return (
              <NTag size="small" type="info" bordered={false}>
                下载中
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
          case 'waiting':
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
          case 'partial_error':
            return (
              <NTooltip>
                {{
                  trigger: () => (
                    <NTag size="small" type="error" bordered={false}>
                      下载失败
                    </NTag>
                  ),
                  default: () => row.errorMessage || '未知错误',
                }}
              </NTooltip>
            );
          case 'verify_failed':
            return (
              <NTooltip>
                {{
                  trigger: () => (
                    <NTag size="small" type="error" bordered={false}>
                      校验失败
                    </NTag>
                  ),
                  default: () => row.errorMessage || 'SHA1校验失败',
                }}
              </NTooltip>
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
        if (row.status === 'active')
          return <NProgress type="line" percentage={Math.floor(row.progress || 0)} processing />;
        if (row.status === 'pausing' || row.status === 'paused')
          return (
            <NProgress type="line" percentage={Math.floor(row.progress || 0)} status="warning" />
          );
        return '';
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
      title: '操作',
      key: 'action',
      width: 280,
      render: (row) => {
        return (
          <NSpace>
            {(() => {
              if ((row.status === 'active' || row.status === 'pausing') && !row.isCollecting) {
                return (
                  <NButton
                    text
                    type="warning"
                    disabled={row.status === 'pausing'}
                    onClick={async () => {
                      try {
                        if (row.isFolder) {
                          await pauseFolder(row);
                        } else {
                          // 立即设置状态为"暂停中"
                          row.status = 'pausing';
                          await invoke('download_pause_task', { gid: row.gid });
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
                      default: () => (row.status === 'pausing' ? '暂停中' : '暂停'),
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
                          await resumeSingleFile(row);
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
              } else if (
                row.status === 'error' ||
                row.status === 'partial_error' ||
                row.status === 'verify_failed'
              ) {
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
</script>

<style scoped></style>
