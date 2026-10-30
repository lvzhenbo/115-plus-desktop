<template>
  <NEl class="flex flex-col h-[calc(100vh-59px)] text-(--text-color-2)">
    <ExplorerToolbarLayout>
      <NTooltip>
        <template #trigger>
          <NButton
            size="small"
            quaternary
            :disabled="!checkedRowKeys.length"
            @click="handleRestore(checkedRowKeys.join(','))"
          >
            <template #icon
              ><NIcon><RestoreFilled /></NIcon
            ></template>
          </NButton>
        </template>
        批量还原
      </NTooltip>
      <NTooltip>
        <template #trigger>
          <NButton
            size="small"
            quaternary
            type="error"
            :disabled="!checkedRowKeys.length"
            @click="handleDelete(checkedRowKeys.join(','))"
          >
            <template #icon
              ><NIcon><DeleteOutlined /></NIcon
            ></template>
          </NButton>
        </template>
        批量删除
      </NTooltip>
      <NTooltip>
        <template #trigger>
          <NButton size="small" quaternary type="error" @click="handleClearRecycleBin">
            <template #icon
              ><NIcon><ClearOutlined /></NIcon
            ></template>
          </NButton>
        </template>
        清空回收站
      </NTooltip>
    </ExplorerToolbarLayout>
    <ListHeader
      v-if="items.length"
      :items
      :selected-items="selectedItems"
      :show-checkbox="true"
      :columns="['size']"
      sort-disabled
      @toggle-select-all="toggleSelectAll"
    >
      <template #after>
        <div class="w-48 shrink-0 px-2">原位置</div>
        <div class="w-40 shrink-0 px-2">删除时间</div>
        <div class="w-40 shrink-0 px-2">操作</div>
      </template>
    </ListHeader>
    <NEmpty v-if="!items.length && !loading" class="flex-1 flex items-center justify-center" />
    <NSpin v-else :show="loading" class="flex-1 min-h-0 overflow-hidden" content-class="h-full">
      <NScrollbar class="h-full">
        <FileItemView
          v-for="item in items"
          :key="item.fid"
          :item
          :selected="selectedItems.has(item.fid)"
          :show-checkbox="true"
          :columns="['size']"
          view-mode="list"
          @check="toggleRow(item.fid)"
        >
          <template #after>
            <div
              class="w-48 shrink-0 px-2 text-sm text-(--text-color-3) truncate"
              :title="item.source.parent_name"
              >{{ item.source.parent_name }}</div
            >
            <div class="w-40 shrink-0 px-2 text-sm text-(--text-color-3) whitespace-nowrap">{{
              item.source.dtime
                ? format(new Date(Number(item.source.dtime) * 1000), 'yyyy-MM-dd HH:mm:ss')
                : ''
            }}</div>
            <div class="w-40 shrink-0 px-2 flex items-center gap-3" @click.stop>
              <NButton text type="primary" @click="handleRestore(item.fid)">
                <template #icon
                  ><NIcon><RestoreFilled /></NIcon></template
                >还原
              </NButton>
              <NButton text type="error" @click="handleDelete(item.fid)">
                <template #icon
                  ><NIcon><DeleteOutlined /></NIcon></template
                >删除
              </NButton>
            </div>
          </template>
        </FileItemView>
      </NScrollbar>
    </NSpin>
    <ExplorerStatusbar
      :total-count="pagination.itemCount || 0"
      :selected-count="checkedRowKeys.length"
      :current-page="pagination.page || 1"
      :total-pages="totalPages"
      :page-size="pagination.pageSize || 50"
      @update:current-page="handlePageChange"
      @update:page-size="handlePageSizeChange"
    />
  </NEl>
</template>

<script setup lang="ts">
  import ExplorerToolbarLayout from '@/components/FileExplorer/components/ExplorerToolbarLayout/ExplorerToolbarLayout.vue';
  import ListHeader from '@/components/FileExplorer/components/ListHeader/ListHeader.vue';
  import FileItemView from '@/components/FileExplorer/components/FileItemView/FileItemView.vue';
  import ExplorerStatusbar from '@/components/FileExplorer/components/ExplorerStatusbar/ExplorerStatusbar.vue';
  import { deleteRecycleBinFile, recycleBinList, revertFile } from '@/api/file';
  import type { RecycleBinFile } from '@/api/types/file';
  import { format } from 'date-fns';
  import type { PaginationProps } from 'naive-ui';
  import { RestoreFilled } from '@vicons/material';
  import { DeleteOutlined, ClearOutlined } from '@vicons/antd';

  const dialog = useDialog();
  const message = useMessage();
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
  const selectedItems = computed(() => new Set(checkedRowKeys.value));
  const items = computed(() =>
    data.value.map((source) => ({
      fid: source.id,
      fn: source.file_name,
      fc: source.type === '2' ? '0' : '1',
      ico: source.ico || '',
      fs: Number(source.file_size) || 0,
      source,
    })),
  );
  const totalPages = computed(() =>
    Math.max(1, Math.ceil((pagination.itemCount || 0) / (pagination.pageSize || 50))),
  );

  const toggleRow = (id: string) => {
    checkedRowKeys.value = selectedItems.value.has(id)
      ? checkedRowKeys.value.filter((key) => key !== id)
      : [...checkedRowKeys.value, id];
  };
  const toggleSelectAll = () => {
    checkedRowKeys.value = items.value.every((item) => selectedItems.value.has(item.fid))
      ? []
      : items.value.map((item) => item.fid);
  };

  onMounted(() => {
    getFileList();
  });

  onActivated(() => {
    getFileList();
  });

  let listRequest = 0;
  const getFileList = async () => {
    const request = ++listRequest;
    params.offset = (pagination.page! - 1) * pagination.pageSize!;
    loading.value = true;
    try {
      const res = await recycleBinList({ ...params });
      if (request !== listRequest) return;
      const files: RecycleBinFile[] = [];
      for (const item in res.data) {
        if (
          item !== 'offset' &&
          item !== 'limit' &&
          item !== 'count' &&
          item !== 'rb_pass' &&
          res.data[item]
        ) {
          files.push({ ...res.data[item] });
        }
      }
      data.value = files;
      pagination.itemCount = Number(res.data.count);
      checkedRowKeys.value = [];
    } finally {
      if (request === listRequest) loading.value = false;
    }
  };

  const handlePageChange = (page: number) => {
    pagination.page = page;
    getFileList();
  };

  const handlePageSizeChange = (size: number) => {
    pagination.pageSize = size;
    params.limit = size;
    pagination.page = 1;
    getFileList();
  };

  const handleRestore = (tid: string) => {
    dialog.warning({
      title: '确认要还原选中的文件吗？',
      positiveText: '确定',
      negativeText: '取消',
      onPositiveClick: async () => {
        await revertFile({
          tid,
        });
        message.success('还原成功');
        getFileList();
      },
    });
  };

  const handleDelete = (tid: string) => {
    dialog.warning({
      title: '确认要删除选中的文件吗？',
      content: '文件将被彻底删除且不可恢复',
      positiveText: '确定',
      negativeText: '取消',
      onPositiveClick: async () => {
        await deleteRecycleBinFile({
          tid,
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
