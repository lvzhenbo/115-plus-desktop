<template>
  <div class="relative">
    <!-- 拖拽遮罩层 -->
    <NEl
      v-if="isDragging"
      class="absolute inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm"
    >
      <div
        class="flex flex-col items-center rounded-2xl px-14 py-12 border-2 border-dashed border-(--primary-color) bg-(--modal-color) shadow-(--box-shadow-2) gap-3"
      >
        <NIcon size="56" color="var(--primary-color)">
          <CloudUploadOutlined />
        </NIcon>
        <NText type="primary" strong class="text-lg">释放文件以上传到当前目录</NText>
        <NText depth="3" class="text-sm">文件 / 文件夹均支持</NText>
      </div>
    </NEl>

    <FileExplorer
      ref="explorerRef"
      v-model:cid="cid"
      v-model:view-mode="userStore.homeViewMode"
      v-model:sort-config="userStore.homeSortConfig"
      class="h-[calc(100vh-59px)]"
      @download="handleDownload"
      @batch-download="handleBatchDownload"
      @upload-file="handleUploadFiles"
      @upload-folder="handleUploadFolder"
      @open-file="handleOpenFile"
    />
    <NImageGroup
      v-model:show="imgPreviewVisible"
      v-model:current="imgPreviewIndex"
      :src-list="imgPreviewList"
      :render-toolbar="renderToolbar"
    />
  </div>
</template>

<script setup lang="ts">
  import type { ImageRenderToolbarProps } from 'naive-ui';
  import { open } from '@tauri-apps/plugin-dialog';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { emit, listen } from '@tauri-apps/api/event';
  import { CloudUploadOutlined } from '@vicons/antd';
  import type { MyFile } from '@/api/types/file';
  import { useDownloadManager } from '@/composables/useDownloadManager';
  import { useUploadManager } from '@/composables/useUploadManager';
  import { useUserStore } from '@/store/user';
  import { useSettingStore } from '@/store/setting';

  const route = useRoute();
  const message = useMessage();
  const userStore = useUserStore();
  const settingStore = useSettingStore();
  const explorerRef = useTemplateRef('explorerRef');
  const cid = ref('0');
  const imgPreviewVisible = ref(false);
  const imgPreviewList = ref<string[]>([]);
  const imgPreviewIndex = ref(0);
  const isDragging = ref(false);

  const { download: downloadFile, batchDownload: batchDownloadFiles } = useDownloadManager();

  const { uploadFiles: uploadFilesToCloud, uploadFolder: uploadFolderToCloud } = useUploadManager();

  const selectFile = ref<MyFile | null>(null);

  const unlisten = listen('get-video-list', () => {
    emit('set-video-list', selectFile.value);
  });

  // ============ 拖拽上传 ============

  const unlistenDragDrop = getCurrentWindow().onDragDropEvent(async (event) => {
    if (route.name !== 'Home') return;
    if (event.payload.type === 'over') {
      isDragging.value = true;
    } else if (event.payload.type === 'leave') {
      isDragging.value = false;
    } else if (event.payload.type === 'drop') {
      isDragging.value = false;
      const paths = event.payload.paths;
      if (paths && paths.length > 0) {
        await uploadFilesFromPaths(paths);
      }
    }
  });

  onUnmounted(() => {
    unlisten.then((f) => f());
    unlistenDragDrop.then((f) => f());
  });

  watch(
    route,
    () => {
      if (route.name === 'Home') {
        explorerRef.value?.navigate(route.query.fid?.toString());
      }
    },
    { deep: true },
  );

  // ============ 打开文件 ============

  const handleOpenFile = async (file: MyFile) => {
    selectFile.value = file;
    if (file.isv) {
      try {
        const existingWindow = await WebviewWindow.getByLabel('video-player');
        if (existingWindow) {
          emit('set-video-list', file);
          await existingWindow.setFocus();
        } else {
          const videoPlayerWindow = new WebviewWindow('video-player', {
            url: '/videoPlayer',
            title: file.fn,
            width: 1280,
            height: 720,
            minWidth: 1280,
            minHeight: 720,
            center: true,
            visible: false,
          });

          videoPlayerWindow.once('tauri://error', (e) => {
            console.error('窗口创建失败', e);
            message.error('视频窗口创建失败');
          });
        }
      } catch (e) {
        console.error(e);
      }
    } else if (file.uo) {
      // 收集当前目录下所有图片文件，支持多图预览
      const allItems = explorerRef.value?.getItems() || [];
      const imageFiles = allItems.filter((f) => f.uo);
      if (imageFiles.length > 0) {
        imgPreviewList.value = imageFiles.map((f) => f.uo!);
        const idx = imageFiles.findIndex((f) => f.fid === file.fid);
        imgPreviewIndex.value = idx >= 0 ? idx : 0;
      } else {
        // 兜底：至少预览当前点击的图片
        imgPreviewList.value = [file.uo];
        imgPreviewIndex.value = 0;
      }
      imgPreviewVisible.value = true;
    }
  };

  // ============ 下载 ============

  const handleDownload = async (file: MyFile) => {
    try {
      let targetPath: string | undefined;
      if (settingStore.downloadSetting.askSavePath) {
        const dir = await open({
          directory: true,
          multiple: false,
          title: '选择保存目录',
          defaultPath: settingStore.downloadSetting.downloadPath || undefined,
        });
        if (!dir) return;
        targetPath = `${dir}/${file.fn}`;
      }
      message.info('正在获取下载链接，可在下载列表中查看下载进度');
      await downloadFile(file, targetPath);
    } catch (error) {
      console.error(error);
      message.error('下载任务添加失败');
    }
  };

  const handleBatchDownload = async (files: MyFile[]) => {
    if (files.length === 0) return;
    try {
      let targetDir: string | undefined;
      if (settingStore.downloadSetting.askSavePath) {
        const dir = await open({
          directory: true,
          multiple: false,
          title: '选择保存目录',
          defaultPath: settingStore.downloadSetting.downloadPath || undefined,
        });
        if (!dir) return;
        targetDir = dir;
      }
      message.info(`正在添加 ${files.length} 个文件到下载队列，可在下载列表中查看进度`);
      await batchDownloadFiles(files, targetDir);
    } catch (error) {
      console.error(error);
      message.error('批量下载任务添加失败');
    }
  };

  // ============ 上传 ============

  const handleUploadFiles = async () => {
    const selected = await open({
      multiple: true,
      title: '选择要上传的文件',
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    if (paths.length === 0) return;
    await uploadFilesFromPaths(paths);
  };

  const handleUploadFolder = async () => {
    const selected = await open({
      directory: true,
      title: '选择要上传的文件夹',
    });
    if (!selected) return;

    const folderPath = Array.isArray(selected) ? selected[0] : selected;
    if (!folderPath) return;

    const folderName = folderPath.split(/[\\/]/).pop() || folderPath;

    message.info(`正在添加文件夹 "${folderName}" 到上传队列，可在上传列表中查看进度`);
    try {
      await uploadFolderToCloud(folderPath, folderName, cid.value || '0');
    } catch (error) {
      console.error(error);
      message.error('上传任务添加失败');
    }
  };

  const uploadFilesFromPaths = async (paths: string[], targetCid?: string) => {
    const cidToUse = targetCid || cid.value || '0';
    const files: { path: string; name: string; size: number }[] = [];
    const folders: { path: string; name: string }[] = [];

    for (const filePath of paths) {
      try {
        const isDir: boolean = await invoke('upload_is_directory', { filePath });
        const name = filePath.split(/[\\/]/).pop() || filePath;
        if (isDir) {
          folders.push({ path: filePath, name });
        } else {
          const size: number = await invoke('upload_get_file_size', { filePath });
          files.push({ path: filePath, name, size });
        }
      } catch (e) {
        console.error('获取文件信息失败:', filePath, e);
      }
    }

    if (files.length > 0) {
      message.info(`正在添加 ${files.length} 个文件到上传队列，可在上传列表中查看进度`);
      try {
        await uploadFilesToCloud(files, cidToUse);
      } catch (error) {
        console.error(error);
        message.error('上传任务添加失败');
      }
    }

    for (const folder of folders) {
      message.info(`正在添加文件夹 "${folder.name}" 到上传队列，可在上传列表中查看进度`);
      try {
        await uploadFolderToCloud(folder.path, folder.name, cidToUse);
      } catch (error) {
        console.error(error);
        message.error('上传任务添加失败');
      }
    }
  };

  const renderToolbar = ({ nodes }: ImageRenderToolbarProps) => {
    return [
      nodes.prev,
      nodes.next,
      nodes.rotateCounterclockwise,
      nodes.rotateClockwise,
      nodes.resizeToOriginalSize,
      nodes.zoomOut,
      nodes.zoomIn,
      nodes.close,
    ];
  };
</script>
