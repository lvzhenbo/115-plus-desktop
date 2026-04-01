<template>
  <NEl class="flex flex-col text-(--text-color-2)">
    <!-- 工具栏 -->
    <ExplorerToolbar
      v-if="toolbarActions.length > 0"
      :show="toolbarActions"
      :view-mode="viewMode"
      :loading="loading"
      :has-selection="selectedItems.size > 0"
      :can-go-up="canGoUp"
      @up="goUp"
      @refresh="getFileList"
      @toggle-view="toggleViewMode"
      @new-folder="newFolderModalShow = true"
      @upload-file="handleUploadFiles"
      @upload-folder="handleUploadFolder"
      @batch-download="handleBatchDownload"
      @batch-copy="handleBatchCopy"
      @batch-move="handleBatchMove"
      @batch-rename="handleBatchRename"
      @batch-delete="handleBatchDelete"
    />

    <!-- 面包屑 -->
    <ExplorerBreadcrumb :path="path" :loading="loading" @navigate="handleToFolder" />

    <!-- 主区域 -->
    <div class="flex flex-1 overflow-hidden min-h-0">
      <ExplorerView
        :items="filteredItems"
        :view-mode="viewMode"
        :loading="loading"
        :selected-items="selectedItems"
        :sort-config="sortConfig"
        :show-checkbox="props.showCheckbox"
        :columns="props.columns"
        @click-item="handleItemClick"
        @dblclick-item="handleItemDblClick"
        @contextmenu-item="handleItemContextMenu"
        @contextmenu-bg="handleBgContextMenu"
        @sort="setSort"
        @clear-selection="clearSelection"
        @check-item="handleCheckItem"
        @toggle-select-all="handleToggleSelectAll"
      />
    </div>

    <!-- 状态栏 -->
    <ExplorerStatusbar
      :total-count="pagination.itemCount || 0"
      :selected-count="selectedItems.size"
      :current-page="pagination.page || 1"
      :total-pages="totalPages"
      :page-size="pagination.pageSize || 50"
      @update:current-page="handlePageChange"
      @update:page-size="handlePageSizeChange"
    />

    <!-- 右键菜单 -->
    <ExplorerContextMenu
      v-if="contextMenuActions.length > 0"
      :visible="contextMenuState.visible"
      :x="contextMenuState.x"
      :y="contextMenuState.y"
      :target-item="contextMenuState.targetItem"
      :has-selection="selectedItems.size > 0"
      :show="contextMenuActions"
      @close="closeContextMenu"
      @open="handleOpen"
      @reload="getFileList"
      @download="handleDownload"
      @upload-file="handleUploadFiles"
      @copy="handleContextCopy"
      @move="handleContextMove"
      @rename="renameModalShow = true"
      @batch-rename="handleBatchRename"
      @detail="handleDetail"
      @delete="handleContextDelete"
    />

    <!-- 弹窗 -->
    <DetailModal v-model:show="detailModalShow" :file-detail-data />
    <FolderModal
      v-model:show="folderModalShow"
      :type="folderModalType"
      :ids
      @success="getFileList"
    />
    <RenameModal
      v-model:show="renameModalShow"
      :file="contextMenuState.targetItem"
      @success="getFileList"
    />
    <BatchRenameModal
      v-model:show="batchRenameModalShow"
      :files="batchRenameFiles"
      @success="getFileList"
    />
    <NewFolderModal v-model:show="newFolderModalShow" :pid="params.cid!" @success="getFileList" />
  </NEl>
</template>

<script setup lang="ts">
  import type { PaginationProps } from 'naive-ui';
  import { fileDetail, fileList, deleteFile } from '@/api/file';
  import type {
    FileDetail,
    FileListRequestParams,
    MyFile,
    Path,
    SortField,
  } from '@/api/types/file';
  import type { ViewMode, SortConfig, ToolbarAction, ContextMenuAction, ListColumn } from './types';
  import { useSettingStore } from '@/store/setting';

  const dialog = useDialog();
  const message = useMessage();
  const settingStore = useSettingStore();

  const allToolbarActions: ToolbarAction[] = [
    'up',
    'refresh',
    'newFolder',
    'upload',
    'download',
    'copy',
    'move',
    'rename',
    'delete',
    'viewToggle',
  ];

  const allContextMenuActions: ContextMenuAction[] = [
    'open',
    'reload',
    'download',
    'uploadFile',
    'copy',
    'move',
    'rename',
    'batchRename',
    'detail',
    'delete',
  ];

  const props = withDefaults(
    defineProps<{
      showCheckbox?: boolean;
      onlyFolder?: boolean;
      toolbar?: boolean | ToolbarAction[];
      contextMenu?: boolean | ContextMenuAction[];
      columns?: ListColumn[];
    }>(),
    {
      showCheckbox: true,
      onlyFolder: false,
      toolbar: true,
      contextMenu: true,
      columns: () => ['size', 'type', 'createTime', 'modifyTime'],
    },
  );

  const toolbarActions = computed(() => {
    if (props.toolbar === false) return [];
    if (props.toolbar === true) return allToolbarActions;
    return props.toolbar;
  });

  const contextMenuActions = computed(() => {
    if (props.contextMenu === false) return [];
    if (props.contextMenu === true) return allContextMenuActions;
    return props.contextMenu;
  });

  const emit = defineEmits<{
    download: [file: MyFile];
    'batch-download': [files: MyFile[]];
    'upload-file': [];
    'upload-folder': [];
    'open-file': [file: MyFile];
  }>();

  const cid = defineModel<string>('cid', { default: '0' });
  const viewMode = defineModel<ViewMode>('viewMode', { default: 'list' });
  const sortConfig = defineModel<SortConfig>('sortConfig', {
    default: { field: 'user_utime', direction: 'desc' },
  });

  onMounted(() => {
    getFileList();
  });

  // ============ 状态 ============

  const loading = ref(false);
  const data = ref<MyFile[]>([]);
  const path = ref<Path[]>([]);
  const selectedItems = ref<Set<string>>(new Set());
  const lastClickedId = ref<string | null>(null);
  const forderTemp = ref(new Map<string, number>());

  const pagination = reactive<PaginationProps>({
    page: 1,
    itemCount: 0,
    pageSize: 50,
  });

  const params = reactive<FileListRequestParams>({
    cid: cid.value,
    show_dir: 1,
    offset: 0,
    limit: pagination.pageSize,
    o: sortConfig.value.field,
    asc: sortConfig.value.direction === 'asc' ? 1 : 0,
    custom_order: settingStore.generalSetting.customOrder,
    nf: props.onlyFolder ? 1 : 0,
  });

  // 右键菜单
  const contextMenuState = ref({
    visible: false,
    x: 0,
    y: 0,
    targetItem: null as MyFile | null,
  });

  // 弹窗状态
  const detailModalShow = ref(false);
  const fileDetailData = ref<FileDetail | null>(null);
  const folderModalShow = ref(false);
  const folderModalType = ref<'copy' | 'move'>('copy');
  const renameModalShow = ref(false);
  const batchRenameModalShow = ref(false);
  const batchRenameFiles = ref<MyFile[]>([]);
  const newFolderModalShow = ref(false);
  const ids = ref('');

  // ============ 计算属性 ============

  const filteredItems = computed(() => data.value);

  const totalPages = computed(() =>
    Math.max(1, Math.ceil((pagination.itemCount || 0) / (pagination.pageSize || 50))),
  );

  const canGoUp = computed(() => params.cid !== '0');

  function goUp() {
    handleToFolder(path.value[path.value.length - 2]?.cid ?? '0');
  }

  // ============ 数据加载 ============

  const getFileList = async () => {
    if (params.cid) forderTemp.value.set(params.cid, pagination.page!);
    cid.value = params.cid || '0';
    params.offset = (pagination.page! - 1) * pagination.pageSize!;
    loading.value = true;
    try {
      const res = await fileList({ ...params });
      data.value = res.data;
      pagination.itemCount = res.count;
      path.value = res.path;
      // 记忆排序时，根据接口返回的排序信息更新展示
      if (settingStore.generalSetting.customOrder === 0) {
        sortConfig.value = {
          field: res.order,
          direction: res.is_asc === 1 ? 'asc' : 'desc',
        };
      }
      clearSelection();
    } finally {
      loading.value = false;
    }
  };

  // ============ 视图控制 ============

  function toggleViewMode() {
    viewMode.value = viewMode.value === 'grid' ? 'list' : 'grid';
  }

  function setSort(field: SortField) {
    if (sortConfig.value.field === field) {
      sortConfig.value = {
        field,
        direction: sortConfig.value.direction === 'asc' ? 'desc' : 'asc',
      };
    } else {
      sortConfig.value = { field, direction: 'asc' };
    }
    params.o = sortConfig.value.field;
    params.asc = sortConfig.value.direction === 'asc' ? 1 : 0;
    getFileList();
  }

  // ============ 选择 ============

  function toggleSelect(item: MyFile, multi = false) {
    if (multi) {
      if (selectedItems.value.has(item.fid)) {
        selectedItems.value.delete(item.fid);
      } else {
        selectedItems.value.add(item.fid);
      }
      lastClickedId.value = item.fid;
    } else {
      const wasOnlySelected = selectedItems.value.has(item.fid) && selectedItems.value.size === 1;
      selectedItems.value.clear();
      if (!wasOnlySelected) {
        selectedItems.value.add(item.fid);
        lastClickedId.value = item.fid;
      } else {
        lastClickedId.value = null;
      }
    }
    selectedItems.value = new Set(selectedItems.value);
  }

  function rangeSelect(item: MyFile, additive = false) {
    const list = filteredItems.value;
    const currentIndex = list.findIndex((i) => i.fid === item.fid);
    const anchorIndex = lastClickedId.value
      ? list.findIndex((i) => i.fid === lastClickedId.value)
      : -1;

    if (anchorIndex === -1 || currentIndex === -1) {
      toggleSelect(item, additive);
      return;
    }

    const start = Math.min(anchorIndex, currentIndex);
    const end = Math.max(anchorIndex, currentIndex);
    const rangeIds = list.slice(start, end + 1).map((i) => i.fid);

    if (additive) {
      const merged = new Set(selectedItems.value);
      for (const id of rangeIds) merged.add(id);
      selectedItems.value = merged;
    } else {
      selectedItems.value = new Set(rangeIds);
    }
  }

  function selectAll() {
    selectedItems.value = new Set(filteredItems.value.map((i) => i.fid));
  }

  function clearSelection() {
    selectedItems.value = new Set();
    lastClickedId.value = null;
  }

  function getSelectedFiles(): MyFile[] {
    return data.value.filter((i) => selectedItems.value.has(i.fid));
  }

  // ============ 事件处理 ============

  function handleItemClick(item: MyFile, event: MouseEvent) {
    if (event.shiftKey) {
      rangeSelect(item, event.ctrlKey || event.metaKey);
    } else {
      toggleSelect(item, event.ctrlKey || event.metaKey);
    }
  }

  function handleCheckItem(item: MyFile, event: MouseEvent) {
    if (event.shiftKey) {
      rangeSelect(item, true);
    } else {
      toggleSelect(item, true);
    }
  }

  function handleToggleSelectAll() {
    const allChecked =
      filteredItems.value.length > 0 &&
      filteredItems.value.every((i) => selectedItems.value.has(i.fid));
    if (allChecked) {
      clearSelection();
    } else {
      selectAll();
    }
  }

  function handleItemDblClick(item: MyFile) {
    contextMenuState.value.targetItem = item;
    handleOpen();
  }

  function handleItemContextMenu(item: MyFile, event: MouseEvent) {
    if (!selectedItems.value.has(item.fid)) {
      toggleSelect(item, false);
    }
    contextMenuState.value = {
      visible: true,
      x: event.clientX,
      y: event.clientY,
      targetItem: item,
    };
  }

  function handleBgContextMenu(event: MouseEvent) {
    clearSelection();
    contextMenuState.value = {
      visible: true,
      x: event.clientX,
      y: event.clientY,
      targetItem: null,
    };
  }

  function closeContextMenu() {
    contextMenuState.value.visible = false;
  }

  // ============ 导航 ============

  const handleToFolder = (cid: string) => {
    params.cid = cid.toString();
    pagination.page = forderTemp.value.get(cid) || 1;
    getFileList();
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

  // ============ 文件操作 ============

  const handleOpen = async () => {
    const file = contextMenuState.value.targetItem;
    if (!file) return;

    if (file.fc === '0') {
      params.cid = file.fid;
      pagination.page = forderTemp.value.get(file.fid) || 1;
      getFileList();
    } else {
      emit('open-file', file);
    }
  };

  const handleDownload = () => {
    const file = contextMenuState.value.targetItem;
    if (!file) return;
    emit('download', file);
  };

  const handleBatchDownload = () => {
    const selectedFiles = getSelectedFiles();
    if (selectedFiles.length === 0) return;
    emit('batch-download', selectedFiles);
  };

  const handleContextCopy = () => {
    if (!contextMenuState.value.targetItem) return;
    ids.value = contextMenuState.value.targetItem.fid;
    handleOpenFolderModal('copy');
  };

  const handleContextMove = () => {
    if (!contextMenuState.value.targetItem) return;
    ids.value = contextMenuState.value.targetItem.fid;
    handleOpenFolderModal('move');
  };

  const handleContextDelete = () => {
    if (!contextMenuState.value.targetItem) return;
    ids.value = contextMenuState.value.targetItem.fid;
    handleDelete();
  };

  const handleBatchCopy = () => {
    ids.value = Array.from(selectedItems.value).join(',');
    handleOpenFolderModal('copy');
  };

  const handleBatchMove = () => {
    ids.value = Array.from(selectedItems.value).join(',');
    handleOpenFolderModal('move');
  };

  const handleBatchRename = () => {
    batchRenameFiles.value = getSelectedFiles();
    if (batchRenameFiles.value.length === 0) return;
    batchRenameModalShow.value = true;
  };

  const handleBatchDelete = () => {
    ids.value = Array.from(selectedItems.value).join(',');
    handleDelete();
  };

  const handleOpenFolderModal = (type: 'copy' | 'move') => {
    folderModalType.value = type;
    folderModalShow.value = true;
  };

  const handleDelete = async () => {
    dialog.warning({
      title: '确认要删除选中的文件到回收站？',
      content: '删除的文件可在30天内从回收站还原，回收站仍占用网盘的空间容量哦，请及时清理。',
      positiveText: '确定',
      negativeText: '取消',
      draggable: true,
      onPositiveClick: async () => {
        await deleteFile({ file_ids: ids.value });
        message.success('删除成功');
        getFileList();
      },
    });
  };

  const handleDetail = async () => {
    if (!contextMenuState.value.targetItem) return;
    const res = await fileDetail({ file_id: contextMenuState.value.targetItem.fid });
    fileDetailData.value = res.data;
    detailModalShow.value = true;
  };

  // ============ 上传 ============

  const handleUploadFiles = () => {
    emit('upload-file');
  };

  const handleUploadFolder = () => {
    emit('upload-folder');
  };

  // ============ 键盘快捷键 ============

  onKeyStroke(['a', 'A'], (e) => {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      selectAll();
    }
  });

  onKeyStroke('Delete', () => {
    if (selectedItems.value.size > 0) {
      ids.value = Array.from(selectedItems.value).join(',');
      handleDelete();
    }
  });

  onKeyStroke('F5', (e) => {
    e.preventDefault();
    getFileList();
  });

  // ============ 对外暴露 ============

  function navigate(cid?: string) {
    if (cid) {
      params.cid = cid;
      pagination.page = forderTemp.value.get(cid) || 1;
    }
    getFileList();
  }

  function refresh() {
    getFileList();
  }

  defineExpose({ navigate, refresh });
</script>
