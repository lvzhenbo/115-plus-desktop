<template>
  <div class="px-6 py-3">
    <NSpace class="mb-4">
      <NButton type="primary" :disabled="!checkedRowKeys.length" @click="handleBatchRestore">
        <template #icon>
          <NIcon>
            <RestoreFilled />
          </NIcon>
        </template>
        批量还原
      </NButton>
      <NButton type="error" :disabled="!checkedRowKeys.length" @click="handleBatchDelete">
        <NIcon>
          <DeleteOutlined />
        </NIcon>
        批量删除
      </NButton>
      <NButton type="error" secondary @click="handleClearRecycleBin">
        <NIcon>
          <ClearOutlined />
        </NIcon>
        清空回收站
      </NButton>
    </NSpace>
    <NDataTable
      ref="tableRef"
      v-model:checked-row-keys="checkedRowKeys"
      remote
      flex-height
      :columns
      :data
      :pagination
      :row-key="(row: RecycleBinFile) => row.id"
      :loading
      class="h-[calc(100vh-133px)]"
      @update:page="handlePageChange"
    />
  </div>
</template>

<script setup lang="tsx">
  import { deleteRecycleBinFile, recycleBinList, revertFile } from '@/api/file';
  import type { RecycleBinFile } from '@/api/types/file';
  import { format } from 'date-fns';
  import { filesize } from 'filesize';
  import type { DataTableColumns, DataTableInst, PaginationProps } from 'naive-ui';
  import { RestoreFilled } from '@vicons/material';
  import { DeleteOutlined, ClearOutlined } from '@vicons/antd';

  const dialog = useDialog();
  const message = useMessage();
  const tableRef = ref<DataTableInst | null>(null);
  const checkedRowKeys = ref<string[]>([]);
  const pagination = reactive<PaginationProps>({
    page: 1,
    itemCount: 0,
    pageSize: 50,
  });
  const params = reactive({
    offset: 0,
    limit: pagination.pageSize!,
  });
  const loading = ref(false);
  const data = ref<RecycleBinFile[]>([]);
  const columns: DataTableColumns<RecycleBinFile> = [
    {
      type: 'selection',
    },
    {
      title: '文件名',
      key: 'file_name',
      ellipsis: {
        tooltip: {
          width: 'trigger',
        },
      },
    },
    {
      title: '原位置',
      key: 'parent_name',
      ellipsis: {
        tooltip: {
          width: 'trigger',
        },
      },
    },
    {
      title: '大小',
      key: 'fs',
      width: 100,
      render(row) {
        return row.file_size ? filesize(Number(row.file_size), { standard: 'jedec' }) : '';
      },
    },
    {
      title: '种类',
      key: 'type',
      width: 100,
      render(row) {
        if (row.type === '2') {
          return '文件夹';
        } else if (row.type === '1') {
          return `${row.ico}文件`;
        }
      },
    },
    {
      title: '删除时间',
      key: 'dtime',
      width: 170,
      render(row) {
        return row.dtime ? format(new Date(Number(row.dtime) * 1000), 'yyyy-MM-dd HH:mm:ss') : '';
      },
    },
    {
      title: '操作',
      key: 'action',
      width: 150,
      render: (row) => {
        return (
          <NSpace>
            <NButton
              text
              type="primary"
              onClick={async () => {
                dialog.warning({
                  title: '确认要还原选中的文件吗？',
                  positiveText: '确定',
                  negativeText: '取消',
                  onPositiveClick: async () => {
                    await revertFile({
                      tid: row.id,
                    });
                    message.success('还原成功');
                    getFileList();
                  },
                });
              }}
            >
              {{
                icon: () => (
                  <NIcon>
                    <RestoreFilled />
                  </NIcon>
                ),
                default: () => '还原',
              }}
            </NButton>
            <NButton
              text
              type="error"
              onClick={() => {
                dialog.warning({
                  title: '确认要删除选中的文件吗？',
                  content: '文件将被彻底删除且不可恢复',
                  positiveText: '确定',
                  negativeText: '取消',
                  onPositiveClick: async () => {
                    await deleteRecycleBinFile({
                      tid: row.id,
                    });
                    message.success('删除成功');
                    getFileList();
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

  onMounted(() => {
    getFileList();
  });

  onActivated(() => {
    getFileList();
  });

  const getFileList = async () => {
    params.offset = (pagination.page! - 1) * pagination.pageSize!;
    loading.value = true;
    const res = await recycleBinList({
      ...params,
    });
    data.value = [];
    for (const item in res.data) {
      if (
        item !== 'offset' &&
        item !== 'limit' &&
        item !== 'count' &&
        item !== 'rb_pass' &&
        res.data[item]
      ) {
        data.value.push({
          ...res.data[item],
        });
      }
    }
    pagination.itemCount = Number(res.data.count);
    checkedRowKeys.value = [];
    loading.value = false;
  };

  const handlePageChange = (page: number) => {
    pagination.page = page;
    getFileList();
  };

  const handleBatchRestore = async () => {
    dialog.warning({
      title: '确认要还原选中的文件吗？',
      positiveText: '确定',
      negativeText: '取消',
      onPositiveClick: async () => {
        await revertFile({
          tid: checkedRowKeys.value.join(','),
        });
        message.success('还原成功');
        getFileList();
      },
    });
  };

  const handleBatchDelete = async () => {
    dialog.warning({
      title: '确认要删除选中的文件吗？',
      content: '文件将被彻底删除且不可恢复',
      positiveText: '确定',
      negativeText: '取消',
      onPositiveClick: async () => {
        await deleteRecycleBinFile({
          tid: checkedRowKeys.value.join(','),
        });
        message.success('删除成功');
        getFileList();
      },
    });
  };

  const handleClearRecycleBin = async () => {
    dialog.warning({
      title: '确认要清空回收站吗？',
      content: '文件将被彻底删除且不可恢复',
      positiveText: '确定',
      negativeText: '取消',
      onPositiveClick: async () => {
        await deleteRecycleBinFile();
        message.success('清空成功');
        getFileList();
      },
    });
  };
</script>

<style scoped></style>
