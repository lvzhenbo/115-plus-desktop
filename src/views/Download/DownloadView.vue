<template>
  <div class="px-6 py-3">
    <NSpace class="mb-4">
      <NButton type="primary" @click="handleClear">
        <template #icon>
          <NIcon>
            <ClearOutlined />
          </NIcon>
        </template>
        清除已完成
      </NButton>
    </NSpace>
    <NDataTable
      ref="tableRef"
      remote
      flex-height
      :columns
      :data="settingStore.downloadSetting.downloadList"
      :row-key="(row: DownLoadFile) => row.gid"
      class="h-[calc(100vh-133px)]"
    />
  </div>
</template>

<script setup lang="tsx">
  import { pause, purgeDownloadResult, remove, removeDownloadResult, unpause } from '@/api/aria2';
  import { useSettingStore, type DownLoadFile } from '@/store/setting';
  import {
    DeleteOutlined,
    FolderOutlined,
    ClearOutlined,
    PauseCircleOutlined,
    PlayCircleOutlined,
  } from '@vicons/antd';
  import { filesize } from 'filesize';
  import type { DataTableColumns } from 'naive-ui';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';

  const settingStore = useSettingStore();
  const message = useMessage();
  const dialog = useDialog();
  const columns: DataTableColumns<DownLoadFile> = [
    {
      title: '文件名',
      key: 'name',
      ellipsis: {
        tooltip: {
          width: 'trigger',
        },
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
      width: 120,
      render(row) {
        if (row.status === 'active') {
          return row.downloadSpeed
            ? filesize(row.downloadSpeed, { standard: 'jedec' }) + '/s'
            : '0B/s';
        }
        return '';
      },
    },
    {
      title: '进度',
      key: 'percentDone',
      width: 300,
      render(row) {
        if (row.status === 'error') {
          return <NText type="error">下载失败</NText>;
        } else if (row.status === 'waiting') {
          return <NText type="warning">等待中</NText>;
        } else if (row.status === 'active') {
          return <NProgress type="line" percentage={row.progress || 0} processing />;
        } else if (row.status === 'paused') {
          return <NText type="info">已暂停</NText>;
        } else if (row.status === 'complete') {
          return <NText type="success">下载完成</NText>;
        }
      },
    },
    {
      title: '操作',
      key: 'action',
      width: 220,
      render: (row, index) => {
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
                        await pause(row.gid);
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
                        await unpause(row.gid);
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
                      if (row.status === 'active') {
                        await remove(row.gid);
                        await removeDownloadResult(row.gid);
                      } else {
                        await removeDownloadResult(row.gid);
                      }
                    } catch (e) {
                      console.error(e);
                    } finally {
                      settingStore.downloadSetting.downloadList.splice(index, 1);
                      message.success('下载任务已删除');
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
      onPositiveClick: async () => {
        try {
          await purgeDownloadResult();
          settingStore.downloadSetting.downloadList =
            settingStore.downloadSetting.downloadList.filter(
              (item) =>
                item.status !== 'complete' && item.status !== 'error' && item.status !== 'removed',
            );
          message.success('下载任务已清除');
        } catch (e) {
          console.error(e);
        }
      },
    });
  };
</script>

<style scoped></style>
