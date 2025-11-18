<template>
  <NModal v-model:show="show" preset="card" class="w-250!" title="搜索">
    <NInputGroup class="mb-2">
      <NSelect v-model:value="searchParams.type" :options="options" class="w-30!" />
      <NInput
        v-model:value="searchParams.search_value"
        placeholder="请输入搜索内容"
        clearable
        @keyup.enter="handleSearch"
      />
      <NButton type="primary" :loading @click="handleSearch">
        <template #icon>
          <NIcon>
            <SearchOutlined />
          </NIcon>
        </template>
        搜索
      </NButton>
      <NButton type="error" :disabled="loading" @click="handleClear">
        <template #icon>
          <NIcon>
            <ClearOutlined />
          </NIcon>
        </template>
        清除
      </NButton>
    </NInputGroup>
    <NDataTable
      ref="tableRef"
      remote
      flex-height
      :columns
      :data
      :row-key="(row: SearchFile) => row.file_id"
      :loading
      class="h-120!"
      @scroll="handleScroll"
    />
  </NModal>
  <DetailModal v-model:show="detailModalShow" :file-detail-data />
</template>

<script setup lang="tsx">
  import type { FileDetail, FileSearchRequestParams, SearchFile } from '@/api/types/file';
  import type { DataTableColumns, SelectOption } from 'naive-ui';
  import { FolderOutlined, InfoCircleOutlined, SearchOutlined, ClearOutlined } from '@vicons/antd';
  import { fileDetail, fileSearch } from '@/api/file';
  import { format } from 'date-fns';

  const show = defineModel('show', {
    type: Boolean,
    default: false,
  });

  const message = useMessage();
  const router = useRouter();
  const searchParams = reactive<FileSearchRequestParams>({
    search_value: '',
    limit: 20,
    offset: 0,
    type: 0,
  });
  const options = ref<SelectOption[]>([
    { label: '全部', value: 0 },
    { label: '文档', value: 1 },
    { label: '图片', value: 2 },
    { label: '音乐', value: 3 },
    { label: '视频', value: 4 },
    { label: '压缩包', value: 5 },
    { label: '应用', value: 6 },
  ]);
  const loading = ref(false);
  const data = ref<SearchFile[]>([]);
  const count = ref(0);
  const detailModalShow = ref(false);
  const fileDetailData = ref<FileDetail | null>(null);
  const columns: DataTableColumns<SearchFile> = [
    {
      title: '文件夹',
      key: 'file_name',
      ellipsis: {
        tooltip: {
          width: 'trigger',
        },
      },
    },
    {
      title: '种类',
      key: 'file_category',
      width: 100,
      render(row) {
        if (row.file_category === '0') {
          return '文件夹';
        } else if (row.file_category === '1') {
          return `${row.ico}文件`;
        }
      },
    },
    {
      title: '创建时间',
      key: 'user_ptime',
      width: 170,
      render(row) {
        return row.user_ptime
          ? format(new Date(Number(row.user_ptime) * 1000), 'yyyy-MM-dd HH:mm:ss')
          : '';
      },
    },
    {
      title: '修改时间',
      key: 'user_utime',
      width: 170,
      render(row) {
        return row.user_utime
          ? format(new Date(Number(row.user_utime) * 1000), 'yyyy-MM-dd HH:mm:ss')
          : '';
      },
    },
    {
      title: '操作',
      key: 'action',
      width: 140,
      render: (row) => {
        return (
          <NSpace>
            <NButton
              text
              type="primary"
              onClick={() => {
                router.push({
                  name: 'Home',
                  query: {
                    fid: row.file_category === '0' ? row.file_id : row.parent_id,
                  },
                });
                show.value = false;
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
              type="info"
              onClick={async () => {
                const res = await fileDetail({
                  file_id: row.file_id,
                });
                fileDetailData.value = res.data;
                detailModalShow.value = true;
              }}
            >
              {{
                icon: () => (
                  <NIcon>
                    <InfoCircleOutlined />
                  </NIcon>
                ),
                default: () => '详情',
              }}
            </NButton>
          </NSpace>
        );
      },
    },
  ];

  const handleSearch = async () => {
    if (!searchParams.search_value) {
      message.warning('请输入搜索内容');
      return;
    }
    if (loading.value) return;
    data.value = [];
    searchParams.offset = 0;
    getList();
  };

  const getList = async () => {
    try {
      loading.value = true;
      const res = await fileSearch(searchParams);
      data.value = [...data.value, ...res.data];
      count.value = res.count;
    } catch (error) {
      console.error('Error during search:', error);
    } finally {
      loading.value = false;
    }
  };

  const handleScroll = (e: Event) => {
    const target = e.target as HTMLDivElement;
    if (target.scrollTop + target.clientHeight >= target.scrollHeight) {
      if (loading.value || count.value <= data.value.length) return;
      if (searchParams.offset !== undefined) {
        searchParams.offset += 20;
        getList();
      }
    }
  };

  const handleClear = () => {
    searchParams.search_value = '';
    searchParams.type = 0;
    searchParams.offset = 0;
    data.value = [];
    count.value = 0;
  };
</script>

<style scoped></style>
