<template>
  <div class="px-6 py-3">
    <NSpace class="mb-1">
      <NButton type="primary" :loading="loading" @click="getFileList">
        <template #icon>
          <NIcon>
            <ReloadOutlined />
          </NIcon>
        </template>
        刷新
      </NButton>
    </NSpace>
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
      v-model:checked-row-keys="checkedRowKeys"
      remote
      flex-height
      :columns
      :data
      :pagination
      :row-key="(row: MyFile) => row.fid"
      :loading
      :row-props
      class="h-[calc(100vh-151px)]"
      @update:page="handlePageChange"
    />
    <NDropdown
      placement="bottom-start"
      trigger="manual"
      :x="x"
      :y="y"
      :options="options"
      :show="showDropdown"
      :on-clickoutside="onClickoutside"
      @select="handleSelect"
    />
    <DetailModal v-model:show="detailModalShow" :file-detail-data />
    <FolderModal
      v-model:show="folderModalShow"
      :type="folderModalType"
      :ids
      @success="getFileList"
    />
    <RenameModal v-model:show="renameModalShow" :file="selectFile" @success="getFileList" />
  </div>
</template>

<script setup lang="tsx">
  import {
    type DataTableInst,
    type DataTableColumns,
    type PaginationProps,
    type DropdownOption,
    NIcon,
    NText,
  } from 'naive-ui';
  import { filesize } from 'filesize';
  import { deleteFile, fileDetail, fileList, fileDownloadUrl } from '@/api/file';
  import type { FileDeatil, FileListRequestParams, MyFile, Path } from '@/api/types/file';
  import { format } from 'date-fns';
  import type { HTMLAttributes } from 'vue';
  import {
    FolderOpenOutlined,
    InfoCircleOutlined,
    ReloadOutlined,
    CopyOutlined,
    DeleteOutlined,
    DownloadOutlined,
  } from '@vicons/antd';
  import { DriveFileMoveOutlined, DriveFileRenameOutlineOutlined } from '@vicons/material';
  import DetailModal from './components/DetailModal/DetailModal.vue';
  import FolderModal from './components/FolderModal/FolderModal.vue';
  import RenameModal from './components/RenameModal/RenameModal.vue';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { emit, listen } from '@tauri-apps/api/event';
  import { addUri } from '@/api/aria2';
  import { useSettingStore } from '@/store/setting';

  const route = useRoute();
  const settingStore = useSettingStore();
  const themeVars = useThemeVars();
  const dialog = useDialog();
  const message = useMessage();
  const tableRef = ref<DataTableInst | null>(null);
  const columns: DataTableColumns<MyFile> = [
    {
      type: 'selection',
    },
    {
      title: '文件名',
      key: 'fn',
      ellipsis: {
        tooltip: true,
      },
    },
    {
      title: '大小',
      key: 'fs',
      width: 100,
      render(row) {
        return row.fs ? filesize(row.fs, { standard: 'jedec' }) : '';
      },
    },
    {
      title: '种类',
      key: 'fc',
      width: 100,
      render(row) {
        if (row.fc === '0') {
          return '文件夹';
        } else if (row.fc === '1') {
          return `${row.ico}文件`;
        }
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
  const pagination = reactive<PaginationProps>({
    page: 1,
    itemCount: 0,
    pageSize: 50,
  });
  const loading = ref(false);
  const data = ref<MyFile[]>([]);
  const checkedRowKeys = ref<string[]>([]);
  const params = reactive<FileListRequestParams>({
    cid: '0',
    show_dir: 1,
    offset: 0,
    limit: pagination.pageSize,
  });
  const path = ref<Path[]>([]);
  const forderTemp = ref(new Map<string, number>());
  const selectFile = ref<MyFile | null>(null);
  const ids = ref<string>('');
  const unlisten = listen('get-video-list', () => {
    emit('set-video-list', selectFile.value);
  });

  onMounted(async () => {
    getFileList();
  });

  onActivated(() => {
    if (route.query.fid) {
      params.cid = route.query.fid.toString();
      pagination.page = forderTemp.value.get(params.cid) || 1;
    }
    getFileList();
  });

  onUnmounted(() => {
    unlisten.then((f) => f());
  });

  const getFileList = async () => {
    if (params.cid) forderTemp.value.set(params.cid, pagination.page!);
    params.offset = (pagination.page! - 1) * pagination.pageSize!;
    loading.value = true;
    const res = await fileList({
      ...params,
    });
    data.value = res.data;
    pagination.itemCount = res.count;
    path.value = res.path;
    checkedRowKeys.value = [];
    loading.value = false;
  };

  const rowProps = (row: MyFile): HTMLAttributes => {
    return {
      onClick(e) {
        if ((e.target as HTMLElement).className !== 'n-checkbox-box__border') {
          if (checkedRowKeys.value.includes(row.fid)) {
            checkedRowKeys.value = checkedRowKeys.value.filter((item) => item !== row.fid);
          } else {
            checkedRowKeys.value.push(row.fid);
          }
        }
      },
      onDblclick: () => {
        selectFile.value = row;
        handleOpen();
      },
      onContextmenu: (e: MouseEvent) => {
        selectFile.value = row;
        e.preventDefault();
        showDropdown.value = false;
        nextTick().then(() => {
          showDropdown.value = true;
          x.value = e.clientX;
          y.value = e.clientY;
        });
      },
    };
  };

  const handleOpen = async () => {
    if (!selectFile.value) return;
    if (selectFile.value.fc === '0') {
      params.cid = selectFile.value.fid;
      pagination.page = forderTemp.value.get(selectFile.value.fid) || 1;
      getFileList();
    } else if (selectFile.value.isv) {
      try {
        const existingWindow = await WebviewWindow.getByLabel('video-player');
        if (existingWindow) {
          // 如果窗口已存在，则发送事件
          emit('set-video-list', selectFile.value);
          // 尝试使窗口获得焦点
          await existingWindow.setFocus();
        } else {
          // 创建一个新的窗口实例
          const videoPlayerWindow = new WebviewWindow('video-player', {
            url: '/videoPlayer',
            title: selectFile.value!.fn,
            width: 1280,
            height: 720,
            minWidth: 1280,
            minHeight: 720,
            center: true,
          });

          // 监听窗口创建完成事件
          videoPlayerWindow.once('tauri://created', () => {
            // 窗口创建后，可以在这里传递参数
          });

          // 捕获可能的错误
          videoPlayerWindow.once('tauri://error', (e) => {
            console.error('窗口创建失败', e);
            message.error('视频窗口创建失败');
          });
        }
      } catch (e) {
        console.error(e);
      }
    }
  };

  const handleToFolder = (cid: string) => {
    params.cid = cid.toString();
    pagination.page = forderTemp.value.get(cid) || 1;
    getFileList();
  };

  const handlePageChange = (page: number) => {
    pagination.page = page;
    getFileList();
  };

  const showDropdown = ref(false);
  const x = ref(0);
  const y = ref(0);
  const options: DropdownOption[] = [
    {
      label: '打开',
      key: 'open',
      icon: () => (
        <NIcon>
          <FolderOpenOutlined />
        </NIcon>
      ),
    },
    {
      label: '下载',
      key: 'download',
      icon: () => (
        <NIcon>
          <DownloadOutlined />
        </NIcon>
      ),
    },
    {
      label: '刷新',
      key: 'reload',
      icon: () => (
        <NIcon>
          <ReloadOutlined />
        </NIcon>
      ),
    },
    {
      label: '复制到',
      key: 'copy',
      icon: () => (
        <NIcon>
          <CopyOutlined />
        </NIcon>
      ),
    },
    {
      label: '移动到',
      key: 'move',
      icon: () => (
        <NIcon>
          <DriveFileMoveOutlined />
        </NIcon>
      ),
    },
    {
      label: '重命名',
      key: 'rename',
      icon: () => (
        <NIcon>
          <DriveFileRenameOutlineOutlined />
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
    {
      label: () => <NText type="error">删除</NText>,
      key: 'delete',
      icon: () => (
        <NIcon color={themeVars.value.errorColor}>
          <DeleteOutlined />
        </NIcon>
      ),
    },
  ];
  const detailModalShow = ref(false);
  const fileDetailData = ref<FileDeatil | null>(null);
  const folderModalShow = ref(false);
  const folderModalType = ref<'copy' | 'move'>('copy');
  const renameModalShow = ref(false);

  const onClickoutside = () => {
    showDropdown.value = false;
  };

  const handleSelect = async (key: string) => {
    showDropdown.value = false;
    switch (key) {
      case 'open':
        handleOpen();
        break;
      case 'download':
        handleDownload();
        break;
      case 'reload':
        getFileList();
        break;
      case 'copy':
        if (!selectFile.value) return;
        ids.value = selectFile.value.fid;
        folderModalType.value = 'copy';
        folderModalShow.value = true;
        break;
      case 'move':
        if (!selectFile.value) return;
        ids.value = selectFile.value.fid;
        folderModalType.value = 'move';
        folderModalShow.value = true;
        break;
      case 'rename':
        if (!selectFile.value) return;
        renameModalShow.value = true;
        break;
      case 'detail':
        await getFileDetail();
        detailModalShow.value = true;
        break;
      case 'delete':
        if (!selectFile.value) return;
        dialog.warning({
          title: '确认要删除选中的文件到回收站？',
          content: '删除的文件可在30天内从回收站还原，回收站仍占用网盘的空间容量哦，请及时清理。',
          positiveText: '确定',
          negativeText: '取消',
          draggable: true,
          onPositiveClick: async () => {
            await deleteFile({
              file_ids: selectFile.value!.fid,
            });
            message.success('删除成功');
            getFileList();
          },
        });
        break;
      default:
        break;
    }
  };

  const getFileDetail = async () => {
    const res = await fileDetail({
      file_id: selectFile.value!.fid,
    });
    fileDetailData.value = res.data;
  };

  const handleDownload = async () => {
    if (!selectFile.value) return;
    if (selectFile.value.fc === '1') {
      const res = await fileDownloadUrl({
        pick_code: selectFile.value.pc,
      });
      const aria2res = await addUri(
        res.data[selectFile.value.fid].url.url,
        res.data[selectFile.value.fid].file_name,
      );
      if (aria2res.result) {
        settingStore.downloadSetting.downloadList.unshift({
          name: selectFile.value.fn,
          fid: selectFile.value.fid,
          pickCode: selectFile.value.pc,
          size: selectFile.value.fs,
          gid: aria2res.result,
        });
      }
    }
  };
</script>

<style scoped></style>
