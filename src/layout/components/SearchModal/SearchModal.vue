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
      <NButton type="error" @click="handleClear">
        <template #icon>
          <NIcon>
            <ClearOutlined />
          </NIcon>
        </template>
        清除
      </NButton>
    </NInputGroup>
    <NEl class="h-120 flex flex-col overflow-hidden text-(--text-color-2)">
      <ListHeader
        v-if="items.length"
        :items
        :selected-items="selectedItems"
        :show-checkbox="false"
        :columns
        sort-disabled
      />
      <NEmpty v-if="!items.length && !loading" class="flex-1 flex items-center justify-center" />
      <NSpin v-else :show="loading" class="flex-1 min-h-0 overflow-hidden" content-class="h-full">
        <NScrollbar class="h-full" @scroll="handleScroll">
          <FileItemView
            v-for="item in items"
            :key="item.fid"
            :item
            :selected="false"
            :show-checkbox="false"
            :columns
            view-mode="list"
            @contextmenu="(_item, event) => handleContextMenu(item.source, event)"
          >
            <template #name>
              <a
                href="#"
                class="text-inherit no-underline"
                :aria-disabled="openingId !== null"
                :aria-busy="openingId === item.fid"
                @click.prevent.stop="openingId === null && handleOpen(item.source)"
              >
                <NHighlight
                  :text="item.fn"
                  :patterns="[activeQuery?.search_value || '']"
                  auto-escape
                />
              </a>
            </template>
          </FileItemView>
        </NScrollbar>
      </NSpin>
    </NEl>
  </NModal>
  <NDropdown
    trigger="manual"
    :show="contextMenu.show"
    :x="contextMenu.x"
    :y="contextMenu.y"
    :options="contextMenuOptions"
    @select="handleContextSelect"
    @clickoutside="closeContextMenu"
  />
  <DetailModal v-model:show="detailModalShow" :file-detail-data />
  <RenameModal
    v-model:show="renameModalShow"
    :file-id="renameTarget?.file_id || ''"
    :file-name="renameTarget?.file_name || ''"
    @success="handleSearch"
  />
</template>

<script setup lang="tsx">
  import type { FileDetail, FileSearchRequestParams, SearchFile } from '@/api/types/file';
  import type { DropdownOption, SelectOption } from 'naive-ui';
  import {
    FolderOutlined,
    InfoCircleOutlined,
    SearchOutlined,
    ClearOutlined,
    EditOutlined,
  } from '@vicons/antd';
  import { fileDetail, fileSearch } from '@/api/file';
  import { useFilePreview } from '@/composables/useFilePreview';
  import { searchPreviewFile } from '@/utils/media';
  import type { ListColumn } from '@/components/FileExplorer/types';
  import FileItemView from '@/components/FileExplorer/components/FileItemView/FileItemView.vue';
  import ListHeader from '@/components/FileExplorer/components/ListHeader/ListHeader.vue';

  const show = defineModel('show', {
    type: Boolean,
    default: false,
  });

  const message = useMessage();
  const router = useRouter();
  const { openFile } = useFilePreview();
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
  const columns: ListColumn[] = ['createTime', 'modifyTime'];
  const selectedItems = new Set<string>();
  const items = computed(() =>
    data.value.map((row) => ({
      fid: row.file_id,
      fn: row.file_name,
      fc: row.file_category,
      ico: row.ico || '',
      uppt: Number(row.user_ptime),
      uet: Number(row.user_utime),
      source: row,
    })),
  );
  const activeQuery = shallowRef<FileSearchRequestParams | null>(null);
  const openingId = ref<string | null>(null);
  let searchVersion = 0;
  let nextOffset = 0;
  const detailModalShow = ref(false);
  const renameModalShow = ref(false);
  const renameTarget = ref<SearchFile | null>(null);
  const fileDetailData = ref<FileDetail | null>(null);
  const contextTarget = shallowRef<SearchFile | null>(null);
  const contextMenu = reactive({ show: false, x: 0, y: 0 });
  const contextMenuOptions = computed<DropdownOption[]>(() => [
    ...(contextTarget.value?.file_category === '1'
      ? [
          {
            label: '打开所在文件夹',
            key: 'folder',
            icon: () => (
              <NIcon>
                <FolderOutlined />
              </NIcon>
            ),
          },
        ]
      : []),
    {
      label: '重命名',
      key: 'rename',
      icon: () => (
        <NIcon>
          <EditOutlined />
        </NIcon>
      ),
    },
    {
      label: '详情',
      key: 'detail',
      icon: () => (
        <NIcon>
          <InfoCircleOutlined />
        </NIcon>
      ),
    },
  ]);

  function handleContextMenu(row: SearchFile, event: MouseEvent) {
    contextTarget.value = row;
    Object.assign(contextMenu, { show: true, x: event.clientX, y: event.clientY });
  }

  function closeContextMenu() {
    contextMenu.show = false;
    contextTarget.value = null;
  }

  async function handleContextSelect(key: string) {
    const row = contextTarget.value;
    closeContextMenu();
    if (!row) return;
    if (key === 'folder') {
      await handleOpenFolder(row);
    } else if (key === 'rename') {
      renameTarget.value = row;
      renameModalShow.value = true;
    } else if (key === 'detail') {
      try {
        const res = await fileDetail({ file_id: row.file_id });
        fileDetailData.value = res.data;
        detailModalShow.value = true;
      } catch (error) {
        console.error('获取文件详情失败:', error);
      }
    }
  }

  /**
   * 打开搜索结果：目录由主页路由承载（URL 变化后 HomeView 驱动列表加载）。
   * 必须等导航完成后再关闭弹窗——弹窗关闭会触发 query 清理（replace），
   * 与本次 push 同帧发出时会取消导航，导致「打开」失效。
   */
  const handleOpenFolder = async (row: SearchFile) => {
    try {
      await router.push({
        name: 'Home',
        query: {
          fid: row.file_category === '0' ? row.file_id : row.parent_id,
        },
      });
    } catch (error) {
      console.error('打开目录失败:', error);
    } finally {
      show.value = false;
    }
  };

  const handleOpen = async (row: SearchFile) => {
    closeContextMenu();
    if (row.file_category === '0') return handleOpenFolder(row);
    if (openingId.value || !activeQuery.value) return;
    const query = { ...activeQuery.value };
    openingId.value = row.file_id;
    try {
      if (await openFile(searchPreviewFile(row, query.type), { kind: 'search', params: query })) {
        show.value = false;
      }
    } catch (error) {
      message.error(error instanceof Error ? error.message : '文件打开失败');
    } finally {
      openingId.value = null;
    }
  };

  const handleSearch = async () => {
    closeContextMenu();
    if (!searchParams.search_value.trim()) {
      message.warning('请输入搜索内容');
      return;
    }
    searchVersion++;
    activeQuery.value = {
      ...searchParams,
      search_value: searchParams.search_value.trim(),
      offset: 0,
    };
    data.value = [];
    count.value = 0;
    nextOffset = 0;
    getList();
  };

  const getList = async () => {
    if (!activeQuery.value) return;
    const version = searchVersion;
    const query = { ...activeQuery.value, offset: nextOffset };
    try {
      loading.value = true;
      const res = await fileSearch(query);
      if (version !== searchVersion) return;
      data.value = [...data.value, ...res.data];
      nextOffset += res.data.length;
      count.value = res.data.length ? Math.min(res.count, 10000) : nextOffset;
    } catch (error) {
      console.error('Error during search:', error);
    } finally {
      if (version === searchVersion) loading.value = false;
    }
  };

  const handleScroll = (e: Event) => {
    closeContextMenu();
    const target = e.target as HTMLDivElement;
    if (target.scrollTop + target.clientHeight >= target.scrollHeight) {
      if (loading.value || !activeQuery.value || nextOffset >= count.value) return;
      getList();
    }
  };

  const handleClear = () => {
    closeContextMenu();
    searchVersion++;
    activeQuery.value = null;
    loading.value = false;
    nextOffset = 0;
    searchParams.search_value = '';
    searchParams.type = 0;
    searchParams.offset = 0;
    data.value = [];
    count.value = 0;
  };

  watch(show, (value) => {
    if (!value) {
      closeContextMenu();
      searchVersion++;
      loading.value = false;
    }
  });
</script>
