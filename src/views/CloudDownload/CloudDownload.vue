<template>
  <div class="p-6">
    <NSpace class="mb-4">
      <NButton type="primary" :loading="loading" @click="getTaskList">
        <template #icon>
          <NIcon>
            <ReloadOutlined />
          </NIcon>
        </template>
        刷新
      </NButton>
    </NSpace>
    <NDataTable
      ref="tableRef"
      remote
      flex-height
      :columns
      :data
      :pagination
      :row-key="(row: Task) => row.info_hash"
      :loading
      class="h-[calc(100vh-157px)]"
      @update:page="handlePageChange"
    />
  </div>
</template>

<script setup lang="tsx">
  import { taskDelete, taskList } from '@/api/cloud';
  import type { Task } from '@/api/types/cloud';
  import { filesize } from 'filesize';
  import {
    NButton,
    NCheckbox,
    NIcon,
    NProgress,
    NSpace,
    NText,
    type DataTableColumns,
    type PaginationProps,
  } from 'naive-ui';
  import { ReloadOutlined, CopyOutlined, DeleteOutlined, FolderOutlined } from '@vicons/antd';

  const router = useRouter();
  const message = useMessage();
  const dialog = useDialog();
  const { copy } = useClipboard();
  const data = ref<Task[]>([]);
  const loading = ref(false);
  const pagination = reactive<PaginationProps>({
    page: 1,
    pageCount: 0,
  });
  const flag = ref<1 | 0>(0);
  const columns: DataTableColumns<Task> = [
    {
      title: '文件名',
      key: 'name',
      ellipsis: {
        tooltip: true,
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
      title: '进度',
      key: 'percentDone',
      width: 300,
      render(row) {
        if (row.status === -1) {
          return <NText type="error">下载失败</NText>;
        } else if (row.status === 0) {
          return <NText type="warning">分配中</NText>;
        } else if (row.status === 1) {
          return <NProgress type="line" percentage={Math.floor(row.percentDone)} processing />;
        } else if (row.status === 2) {
          return <NText type="success">下载完成</NText>;
        }
      },
    },
    {
      title: '操作',
      key: 'action',
      width: 110,
      render: (row) => {
        return (
          <NSpace>
            {row.file_id ? (
              <NButton
                text
                onClick={() =>
                  router.push({
                    name: 'Home',
                    query: {
                      fid: row.file_id,
                    },
                  })
                }
              >
                {{
                  icon: () => (
                    <NIcon>
                      <FolderOutlined />
                    </NIcon>
                  ),
                }}
              </NButton>
            ) : null}
            <NButton
              text
              onClick={async () => {
                await copy(row.url);
                message.success('复制成功！');
              }}
            >
              {{
                icon: () => (
                  <NIcon>
                    <CopyOutlined />
                  </NIcon>
                ),
              }}
            </NButton>
            <NButton
              text
              onClick={() => {
                dialog.warning({
                  title: '是否确认删除该下载任务？',
                  content: () => (
                    <NCheckbox v-model:checked={flag.value} checked-value={1} unchecked-value={0}>
                      删除源文件
                    </NCheckbox>
                  ),
                  positiveText: '确定',
                  negativeText: '取消',
                  onPositiveClick: () => {
                    handleDelete(row.info_hash);
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
              }}
            </NButton>
          </NSpace>
        );
      },
    },
  ];

  onMounted(() => {
    getTaskList();
  });

  onActivated(() => {
    getTaskList();
  });

  const getTaskList = async () => {
    try {
      loading.value = true;
      const res = await taskList({
        page: pagination.page!,
      });
      data.value = res.data.tasks;
      pagination.pageCount = res.data.page_count;
    } catch (error) {
      console.error('Error fetching task list:', error);
    } finally {
      loading.value = false;
    }
  };

  const handlePageChange = (page: number) => {
    pagination.page = page;
    getTaskList();
  };

  const handleDelete = async (info_hash: string) => {
    try {
      await taskDelete({
        info_hash,
        del_source_file: flag.value,
      });
      message.success('删除成功');
      getTaskList();
    } catch (error) {
      console.error('Error deleting task:', error);
    }
  };
</script>

<style scoped></style>
