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
      :columns="['size', 'createTime', 'modifyTime']"
      :starred="starred"
      :show-breadcrumb="!starred && cid !== '0'"
      :toolbar="
        starred
          ? ['refresh', 'download', 'copy', 'move', 'rename', 'delete', 'filter', 'viewToggle']
          : true
      "
      :context-menu="
        starred
          ? [
              'open',
              'reload',
              'download',
              'copy',
              'move',
              'rename',
              'batchRename',
              'detail',
              'delete',
              'toggleStar',
            ]
          : true
      "
      defer-navigation
      class="h-[calc(100vh-59px)]"
      @navigate-request="handleExplorerNavigate"
      @download="handleDownload"
      @batch-download="handleBatchDownload"
      @upload-file="handleUploadFiles"
      @upload-folder="handleUploadFolder"
      @open-file="handleOpenFile"
    />
  </div>
</template>

<script setup lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { CloudUploadOutlined } from '@vicons/antd';
  import type { MyFile } from '@/api/types/file';
  import { useDownloadManager } from '@/composables/useDownloadManager';
  import { useUploadManager } from '@/composables/useUploadManager';
  import { useUserStore } from '@/store/user';
  import { useSettingStore } from '@/store/setting';
  import { useFilePreview } from '@/composables/useFilePreview';

  const route = useRoute();
  const router = useRouter();
  const starred = computed(() => route.query.star === '1');
  const message = useMessage();
  const userStore = useUserStore();
  const settingStore = useSettingStore();
  const explorerRef = useTemplateRef('explorerRef');
  // 目录由路由 query 承载：初次加载时从 URL 恢复，之后的目录切换都会写回 URL，
  // 使浏览器 / 鼠标侧键的回退能逐级返回浏览过的目录。
  const cid = ref(route.query.fid ? String(route.query.fid) : '0');
  const isDragging = ref(false);

  const { download: downloadFile, batchDownload: batchDownloadFiles } = useDownloadManager();

  const { uploadFiles: uploadFilesToCloud, uploadFolder: uploadFolderToCloud } = useUploadManager();

  const { openFile } = useFilePreview();

  // ============ 拖拽上传 ============

  const unlistenDragDrop = getCurrentWindow().onDragDropEvent(async (event) => {
    if (route.name !== 'Home' || starred.value) return;
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
    unlistenDragDrop.then((f) => f());
  });

  // 目录导航委托给路由（见 FileExplorer 的 defer-navigation）：URL 变化后再驱动列表加载。
  // 无 fid 的历史条目对应根目录；从其它页面切回主页（fid 无变化）时不会触发，从而保留当前目录。
  watch(
    () => [route.query.fid, starred.value] as const,
    ([fid, star], [, previousStar]) => {
      if (route.name !== 'Home') return;
      explorerRef.value?.navigate(fid ? String(fid) : '0', star !== previousStar);
    },
    { flush: 'post' },
  );

  /** FileExplorer 请求导航到目标目录：写入路由，由上面的 watch 统一加载。 */
  const handleExplorerNavigate = (targetCid: string) => {
    if (!starred.value && String(route.query.fid ?? '0') === targetCid) return;

    const query = { ...route.query };
    delete query.star;
    if (targetCid === '0') delete query.fid;
    else query.fid = targetCid;
    router.push({ name: 'Home', query });
  };

  // ============ 打开文件 ============

  const handleOpenFile = async (file: MyFile) => {
    const source = explorerRef.value?.getMediaSource();
    if (!source) return;
    try {
      await openFile(file, source);
    } catch (error) {
      message.error(error instanceof Error ? error.message : '文件打开失败');
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
</script>
