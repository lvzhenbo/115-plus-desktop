<template>
  <NModal v-model:show="show" preset="card" class="w-250!" title="选择要保存的目标文件夹">
    <NBreadcrumb class="mb-1">
      <NBreadcrumbItem v-for="item in path" :key="item.cid" @click="handleToFolder(item.cid)">
        <NEllipsis
          class="max-w-60!"
          :tooltip="{
            placement: 'top',
            width: 'trigger',
          }"
        >
          {{ item.name }}
        </NEllipsis>
      </NBreadcrumbItem>
    </NBreadcrumb>
    <NDataTable
      ref="tableRef"
      remote
      flex-height
      :columns
      :data
      :row-key="(row: MyFile) => row.fid"
      :loading
      :row-props
      class="h-120!"
      @scroll="handleScroll"
    />
    <template #action>
      <NSpace justify="space-between">
        <NButton @click="newFolderShow = true">新建文件夹</NButton>
        <NButton type="primary" @click="handleSubmit">保存到这里</NButton>
      </NSpace>
    </template>
  </NModal>
  <NewFolderModal v-model:show="newFolderShow" :pid="params.cid!" @success="handleRefresh" />
</template>

<script setup lang="ts">
  import { fileList } from '@/api/file';
  import type { FileListRequestParams, MyFile, Path } from '@/api/types/file';
  import { useUserStore } from '@/store/user';
  import { format } from 'date-fns';
  import type { DataTableColumns } from 'naive-ui';
  import type { HTMLAttributes } from 'vue';

  const show = defineModel('show', {
    type: Boolean,
    default: false,
  });

  const emits = defineEmits<{
    select: [cid: string];
  }>();

  const userStore = useUserStore();
  const columns: DataTableColumns<MyFile> = [
    {
      title: '文件夹名',
      key: 'fn',
      ellipsis: {
        tooltip: true,
      },
    },
    {
      title: '创建时间',
      key: 'uppt',
      width: 170,
      render(row) {
        return row.uppt ? format(new Date(row.uppt * 1000), 'yyyy-MM-dd HH:mm:ss') : '';
      },
    },
    {
      title: '修改时间',
      key: 'uet',
      width: 170,
      render(row) {
        return row.uet ? format(new Date(row.uet * 1000), 'yyyy-MM-dd HH:mm:ss') : '';
      },
    },
  ];
  const data = ref<MyFile[]>([]);
  const loading = ref(false);
  const params = reactive<FileListRequestParams>({
    cid: '0',
    show_dir: 1,
    offset: 0,
    limit: 50,
    nf: 1,
  });
  const path = ref<Path[]>([]);
  const count = ref(0);
  const newFolderShow = ref(false);
  const message = useMessage();

  watch(show, (val) => {
    if (val) {
      params.cid = userStore.getLatestFolder('save');
      getFileList();
    } else {
      data.value = [];
      path.value = [];
    }
  });

  const getFileList = async () => {
    loading.value = true;
    const res = await fileList({
      ...params,
    });
    data.value = [...data.value, ...res.data];
    path.value = res.path;
    count.value = res.count;
    loading.value = false;
  };

  const handleToFolder = (cid: string) => {
    params.cid = cid.toString();
    handleRefresh();
  };

  const rowProps = (row: MyFile): HTMLAttributes => {
    return {
      onDblclick: () => {
        params.cid = row.fid;
        handleRefresh();
      },
    };
  };

  const handleScroll = (e: Event) => {
    const target = e.target as HTMLDivElement;
    if (target.scrollTop + target.clientHeight >= target.scrollHeight) {
      if (loading.value || count.value <= data.value.length) return;
      if (params.offset !== undefined) {
        params.offset += 50;
        getFileList();
      }
    }
  };

  const handleRefresh = () => {
    params.offset = 0;
    data.value = [];
    count.value = 0;
    getFileList();
  };

  const handleSubmit = async () => {
    try {
      if (!params.cid) {
        message.error('请选择文件夹');
        return;
      }
      emits('select', params.cid);
      show.value = false;
    } catch (error) {
      console.error(error);
    }
  };
</script>

<style scoped></style>
