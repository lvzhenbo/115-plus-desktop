<template>
  <NEl class="flex flex-col text-(--text-color-2)">
    <!-- 工具栏 -->
    <ExplorerToolbar
      v-if="toolbarActions.length > 0"
      v-model:search-keyword="searchKeyword"
      :show="toolbarActions"
      :enable-search="props.enableSearch"
      :view-mode="viewMode"
      :loading="loading"
      :has-selection="selectedItems.size > 0"
      :can-go-up="canGoUp"
      :is-searching="isSearching"
      @up="goUp"
      @refresh="handleRefresh"
      @toggle-view="toggleViewMode"
      @new-folder="newFolderModalShow = true"
      @upload-file="handleUploadFiles"
      @upload-folder="handleUploadFolder"
      @batch-download="handleBatchDownload"
      @batch-copy="handleBatchCopy"
      @batch-move="handleBatchMove"
      @batch-rename="handleBatchRename"
      @batch-delete="handleBatchDelete"
      @search="handleSearch"
    />

    <!-- 面包屑 -->
    <ExplorerBreadcrumb
      :path="path"
      :loading="loading"
      :favorited="currentFolderFavorited"
      :favorite-disabled="favoriteDisabled"
      :show-favorites="props.showFavorites"
      :favorites-open="!userStore.favoritesCollapsed"
      @navigate="handleToFolder"
      @toggle-favorite="toggleFavorite"
      @toggle-favorites="toggleFavoritesPanel"
    />

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
        :sort-disabled="isSearching"
        @click-item="handleItemClick"
        @dblclick-item="handleItemDblClick"
        @contextmenu-item="handleItemContextMenu"
        @contextmenu-bg="handleBgContextMenu"
        @sort="setSort"
        @clear-selection="clearSelection"
        @check-item="handleCheckItem"
        @toggle-select-all="handleToggleSelectAll"
      />
      <ExplorerFavorites
        v-if="props.showFavorites"
        v-model:collapsed="userStore.favoritesCollapsed"
        :current-cid="params.cid || '0'"
        @navigate="handleNavigateToFavorite"
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
      @reload="handleRefresh"
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
      :file-id="contextMenuState.targetItem?.fid || ''"
      :file-name="contextMenuState.targetItem?.fn || ''"
      @success="getFileList"
    />
    <BatchRenameModal
      v-model:show="batchRenameModalShow"
      :files="batchRenameFiles"
      @success="getFileList"
    />
    <NewFolderModal
      v-model:show="newFolderModalShow"
      :pid="params.cid || '0'"
      @success="getFileList"
    />
  </NEl>
</template>

<script setup lang="ts">
  import type { PaginationProps } from 'naive-ui';
  import { fileDetail, fileList, fileSearch, deleteFile } from '@/api/file';
  import type {
    FileDetail,
    FileListRequestParams,
    MyFile,
    Path,
    SearchFile,
    SortField,
  } from '@/api/types/file';
  import type { ViewMode, SortConfig, ToolbarAction, ContextMenuAction, ListColumn } from './types';
  import { useExplorerShortcuts } from '@/composables/useExplorerShortcuts';
  import { useModalQuerySync } from '@/composables/useModalQuerySync';
  import { useSettingStore } from '@/store/setting';
  import { useUserStore } from '@/store/user';

  const dialog = useDialog();
  const message = useMessage();
  const settingStore = useSettingStore();
  const userStore = useUserStore();

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
    'toggleFavorite',
  ];

  const props = withDefaults(
    defineProps<{
      showCheckbox?: boolean;
      onlyFolder?: boolean;
      toolbar?: boolean | ToolbarAction[];
      contextMenu?: boolean | ContextMenuAction[];
      columns?: ListColumn[];
      enableSearch?: boolean;
      showFavorites?: boolean;
      /** 目录导航委托给外部处理（主页会写入路由，由 URL 变化驱动加载），默认直接本地跳转 */
      deferNavigation?: boolean;
    }>(),
    {
      showCheckbox: true,
      onlyFolder: false,
      toolbar: true,
      contextMenu: true,
      columns: () => ['size', 'type', 'createTime', 'modifyTime'],
      enableSearch: false,
      showFavorites: true,
      deferNavigation: false,
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
    'navigate-request': [cid: string];
  }>();

  const cid = defineModel<string>('cid', { default: '0' });
  const viewMode = defineModel<ViewMode>('viewMode', { default: 'list' });
  const sortConfig = defineModel<SortConfig>('sortConfig', {
    default: () => ({ field: 'user_utime', direction: 'desc' }),
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

  // 搜索状态
  const searchKeyword = ref('');
  const isSearching = ref(false);

  // 搜索关键词变化时，清空则退出搜索
  watch(searchKeyword, (val) => {
    if (!val && isSearching.value) {
      exitSearchMode();
      getFileList();
    }
  });

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
    exitSearchMode();
    handleToFolder(path.value[path.value.length - 2]?.cid ?? '0');
  }

  // ============ 数据加载 ============

  const getFileList = async () => {
    if (params.cid) forderTemp.value.set(params.cid, pagination.page || 1);
    cid.value = params.cid || '0';
    params.offset = ((pagination.page || 1) - 1) * (pagination.pageSize || 50);
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

  // ============ 搜索 ============

  /** 将 SearchFile 转换为 MyFile */
  function searchFileToMyFile(sf: SearchFile): MyFile {
    return {
      fid: sf.file_id,
      fn: sf.file_name,
      fc: sf.file_category,
      pid: sf.parent_id,
      ico: sf.ico || '',
      fs: Number(sf.file_size) || 0,
      uppt: Number(sf.user_ptime) || 0,
      upt: Number(sf.user_utime) || 0,
      uet: Number(sf.user_utime) || 0,
      sha1: sf.sha1 || '',
      aid: sf.area_id,
      cm: 0,
      def: 0,
      def2: 0,
      fatr: '',
      fco: '',
      fdesc: '',
      fl: [],
      fta: '1',
      ftype: '',
      fuuid: 0,
      fvs: 0,
      ic: '',
      is_top: 0,
      ism: '0',
      isp: 0,
      ispl: 0,
      iss: 0,
      issct: 0,
      isv: 0,
      multitrack: 0,
      opt: 0,
      pc: '',
      play_long: 0,
      v_img: '',
    };
  }

  function exitSearchMode() {
    isSearching.value = false;
    searchKeyword.value = '';
  }

  /** 执行搜索请求 */
  async function executeSearch(offset: number) {
    loading.value = true;
    try {
      const res = await fileSearch({
        search_value: searchKeyword.value,
        cid: params.cid,
        limit: pagination.pageSize || 50,
        offset,
        fc: 1,
      });
      data.value = res.data.map(searchFileToMyFile);
      pagination.itemCount = res.count;
    } finally {
      loading.value = false;
    }
  }

  async function handleSearch() {
    if (!searchKeyword.value) {
      exitSearchMode();
      getFileList();
      return;
    }
    isSearching.value = true;
    pagination.page = 1;
    await executeSearch(0);
    path.value = [];
    clearSelection();
  }

  async function handleSearchPage() {
    const offset = ((pagination.page || 1) - 1) * (pagination.pageSize || 50);
    await executeSearch(offset);
    clearSelection();
  }

  function handleRefresh() {
    if (isSearching.value) {
      handleSearch();
    } else {
      getFileList();
    }
  }

  // ============ 视图控制 ============

  function toggleViewMode() {
    viewMode.value = viewMode.value === 'grid' ? 'list' : 'grid';
  }

  function setSort(field: SortField) {
    if (isSearching.value) return;

    // 记忆排序由 115 服务端控制：开放平台未提供排序接口，本地排序请求不会生效。
    if (settingStore.generalSetting.customOrder === 0) {
      message.warning(
        '记忆排序模式下无法手动调整排序：115 开放平台未提供排序接口，列表始终按其返回顺序展示。可在「设置 → 排序方式」中切换为「自定义排序」。',
        { duration: 5000 },
      );
      return;
    }

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

  // ============ 收藏 ============

  /** 根目录与搜索模式下不提供收藏入口 */
  const favoriteDisabled = computed(() => (params.cid || '0') === '0' || isSearching.value);

  const currentFolderFavorited = computed(() => {
    const currentCid = params.cid || '0';
    return currentCid !== '0' && userStore.isFavorited(currentCid);
  });

  /** 收藏 / 取消收藏当前所在目录 */
  function toggleFavorite() {
    const currentCid = params.cid || '0';
    if (currentCid === '0' || isSearching.value) return;

    if (userStore.isFavorited(currentCid)) {
      userStore.removeFavorite(currentCid);
      message.success('已取消收藏');
      return;
    }

    const current = path.value[path.value.length - 1];
    const parent = path.value[path.value.length - 2];
    userStore.addFavorite(currentCid, current?.name || '未命名文件夹', parent?.cid || '0');
    message.success('已添加到收藏夹');
  }

  /** 展开 / 收起收藏夹面板 */
  function toggleFavoritesPanel() {
    userStore.favoritesCollapsed = !userStore.favoritesCollapsed;
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

  /**
   * 统一的目录导航入口：
   * - 外部接管（主页路由）时只发出请求，由 URL 变化后再驱动加载；
   * - 其它场景（如弹窗内复用）直接本地跳转。
   */
  const requestFolderNavigation = (cid: string) => {
    exitSearchMode();
    if (props.deferNavigation) {
      emit('navigate-request', cid);
      return;
    }
    navigate(cid);
  };

  const handleToFolder = (cid: string) => {
    requestFolderNavigation(cid);
  };

  function handleNavigateToFavorite(cid: string) {
    handleToFolder(cid);
  }

  const handlePageChange = (page: number) => {
    pagination.page = page;
    if (isSearching.value) {
      handleSearchPage();
    } else {
      getFileList();
    }
  };

  const handlePageSizeChange = (size: number) => {
    pagination.pageSize = size;
    params.limit = size;
    pagination.page = 1;
    if (isSearching.value) {
      handleSearch();
    } else {
      getFileList();
    }
  };

  // ============ 文件操作 ============

  const handleOpen = async () => {
    const file = contextMenuState.value.targetItem;
    if (!file) return;

    if (file.fc === '0') {
      requestFolderNavigation(file.fid);
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

  // ============ 路由联动 ============

  // 仅主页场景（deferNavigation）下让弹窗参与全局回退：
  // 打开弹窗会写入路由 query，侧键 / 快捷键回退时优先关闭最上层弹窗。
  const modalQueryEnabled = () => props.deferNavigation;

  useModalQuerySync(folderModalShow, 'folder', modalQueryEnabled);
  useModalQuerySync(newFolderModalShow, 'newFolder', modalQueryEnabled);
  useModalQuerySync(renameModalShow, 'rename', modalQueryEnabled);
  useModalQuerySync(batchRenameModalShow, 'batchRename', modalQueryEnabled);

  // ============ 键盘快捷键 ============

  // 页面上可能同时存在多个 FileExplorer 实例（例如 FolderModal 弹窗内复用的列表），
  // 快捷键只应由「最上层」实例响应；被弹窗遮挡的背景实例（如离线下载弹窗）同样不再响应。
  const shortcutToken = Symbol('file-explorer');
  const explorerShortcuts = useExplorerShortcuts();

  // 注册顺序即实例创建顺序：后打开的实例（例如弹窗内的列表）自动排在栈顶；
  // 卸载时由 scope 清理兜底注销。
  explorerShortcuts.register(shortcutToken);

  tryOnScopeDispose(() => {
    explorerShortcuts.unregister(shortcutToken);
  });

  /** 组件根 DOM（NEl 渲染出的 div）。 */
  const rootElement = (): HTMLElement | null => {
    const el = getCurrentInstance()?.proxy?.$el;
    return el instanceof HTMLElement ? el : null;
  };

  /** 当前实例是否被已打开的弹窗遮挡（自身位于弹窗内时不算被遮挡）。 */
  const isCoveredByModal = () => {
    const root = rootElement();
    if (!root) return false;
    if (root.closest('.n-modal-container')) return false;

    return document.querySelector('.n-modal-container') !== null;
  };

  /** 事件是否发生在输入类元素内，此类场景不拦截按键（如搜索框内的 Ctrl+A / Delete）。 */
  const isEditingText = (event: KeyboardEvent) => {
    const target = event.target;
    return (
      target instanceof HTMLElement &&
      target.closest('input, textarea, [contenteditable="true"]') !== null
    );
  };

  /** 快捷键是否应由当前实例处理：位于实例栈顶且未被弹窗遮挡。 */
  const shouldHandleShortcut = () => explorerShortcuts.isTop(shortcutToken) && !isCoveredByModal();

  // `dedupe` 让 vueuse 忽略按住按键产生的重复事件（避免 F5 连发请求、Delete 连弹确认框）。
  onKeyStroke(
    ['a', 'A'],
    (e) => {
      if (isEditingText(e) || !shouldHandleShortcut()) return;

      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        selectAll();
      }
    },
    { dedupe: true },
  );

  onKeyStroke(
    'Delete',
    (e) => {
      if (isEditingText(e) || !shouldHandleShortcut()) return;

      if (selectedItems.value.size > 0) {
        ids.value = Array.from(selectedItems.value).join(',');
        handleDelete();
      }
    },
    { dedupe: true },
  );

  onKeyStroke(
    'F5',
    (e) => {
      if (!shouldHandleShortcut()) return;

      e.preventDefault();
      getFileList();
    },
    { dedupe: true },
  );

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

  function getItems(): MyFile[] {
    return data.value;
  }

  defineExpose({ navigate, refresh, getItems });
</script>
