<template>
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
</template>

<script setup lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { emit, listen } from '@tauri-apps/api/event';
  import type { MyFile } from '@/api/types/file';
  import { useDownloadManager } from '@/composables/useDownloadManager';
  import { useUploadManager } from '@/composables/useUploadManager';
  import { useUserStore } from '@/store/user';

  const route = useRoute();
  const message = useMessage();
  const userStore = useUserStore();
  const explorerRef = useTemplateRef('explorerRef');
  const cid = ref('0');

  const {
    init: initDownloadManager,
    download: downloadFile,
    batchDownload: batchDownloadFiles,
  } = useDownloadManager();

  const {
    init: initUploadManager,
    uploadFiles: uploadFilesToCloud,
    uploadFolder: uploadFolderToCloud,
  } = useUploadManager();

  const selectFile = ref<MyFile | null>(null);

  const unlisten = listen('get-video-list', () => {
    emit('set-video-list', selectFile.value);
  });

  onMounted(() => {
    initDownloadManager();
    initUploadManager();
  });

  onUnmounted(() => {
    unlisten.then((f) => f());
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
    }
  };

  // ============ 下载 ============

  const handleDownload = async (file: MyFile) => {
    message.info('正在获取下载链接，可在下载列表中查看下载进度');
    try {
      await downloadFile(file);
    } catch (error) {
      console.error(error);
      message.error('下载任务添加失败');
    }
  };

  const handleBatchDownload = async (files: MyFile[]) => {
    if (files.length === 0) return;
    message.info(`正在添加 ${files.length} 个文件到下载队列，可在下载列表中查看进度`);
    try {
      await batchDownloadFiles(files);
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

    const files: { path: string; name: string; size: number }[] = [];
    for (const filePath of paths) {
      try {
        const size: number = await invoke('get_file_size', { filePath });
        const name = filePath.split(/[\\/]/).pop() || filePath;
        files.push({ path: filePath, name, size });
      } catch (e) {
        console.error('获取文件信息失败:', e);
      }
    }

    if (files.length === 0) return;

    message.info(`正在添加 ${files.length} 个文件到上传队列，可在上传列表中查看进度`);
    try {
      await uploadFilesToCloud(files, cid.value || '0');
    } catch (error) {
      console.error(error);
      message.error('上传任务添加失败');
    }
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
</script>
