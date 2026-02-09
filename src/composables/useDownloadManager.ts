import {
  addUri,
  batchTellStatus,
  forceRemove,
  removeDownloadResult,
  pause as pauseAria2,
  unpause as unpauseAria2,
} from '@/api/aria2';
import { fileDownloadUrl, fileList } from '@/api/file';
import type { MyFile } from '@/api/types/file';
import { useSettingStore, type DownLoadFile } from '@/store/setting';
import {
  insertDownload,
  updateDownload,
  deleteDownload,
  deleteChildDownloads,
  deleteFinishedDownloads,
  getAllDownloads,
  getActiveGids,
  getChildDownloads,
  getIncompleteDownloads,
  getTopLevelDownloads,
} from '@/db/downloads';

/** 下载队列项 */
interface DownloadQueueItem {
  file: MyFile;
  path?: string;
  retryCount: number;
  parentGid?: string;
}

/** 最大重试次数 */
const MAX_RETRY = 5;
/** 获取下载链接之间的最小延迟(ms) */
const FETCH_DELAY = 2000;
/** 文件夹列表请求之间的延迟(ms) */
const FOLDER_LIST_DELAY = 1000;
/** 限流退避基础延迟(ms) */
const BACKOFF_BASE = 3000;
/** 限流退避最大延迟(ms) */
const BACKOFF_MAX = 60000;
/** 状态轮询间隔(ms) */
const POLL_INTERVAL = 2000;
/** 恢复下载时重新获取链接的延迟(ms) */
const RESUME_FETCH_DELAY = 2000;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const isRateLimitError = (error: unknown): boolean => {
  if (!error) return false;
  const err = error as Record<string, unknown>;
  if (err.status === 429 || err.statusCode === 429) return true;
  if (err.code === 20130827 || err.errno === 20130827) return true;
  const msg = String(err.message || err.msg || '');
  if (/rate.?limit|too.?many|频繁|限流|请求过快/i.test(msg)) return true;
  return false;
};

const getBackoffDelay = (retryCount: number): number => {
  const delay = Math.min(BACKOFF_BASE * Math.pow(2, retryCount), BACKOFF_MAX);
  const jitter = delay * 0.25 * (Math.random() * 2 - 1);
  return Math.round(delay + jitter);
};

/**
 * 从 aria2 中清除已终结的任务记录
 */
const cleanupAria2Task = async (gid: string) => {
  if (gid.startsWith('failed-') || gid.startsWith('folder-')) return;
  try {
    await removeDownloadResult(gid);
  } catch {
    // 忽略
  }
};

/**
 * 下载管理器
 *
 * 使用 SQLite 存储下载列表，通过响应式 displayList 驱动 UI。
 * aria2 仅作为下载引擎，不使用 session；完成/失败自动清理 aria2 记录。
 * 应用启动时自动恢复未完成的下载。
 */
export const useDownloadManager = createSharedComposable(() => {
  const settingStore = useSettingStore();

  /** 响应式的顶层下载列表（用于 UI 展示） */
  const displayList = ref<DownLoadFile[]>([]);
  /** 下载队列 */
  const downloadQueue = ref<DownloadQueueItem[]>([]);
  const isProcessing = ref(false);
  const isResuming = ref(false);

  /**
   * 从数据库刷新顶层列表到响应式变量
   */
  const refreshDisplayList = async () => {
    displayList.value = await getTopLevelDownloads();
  };

  /**
   * 状态轮询
   */
  const {
    pause: stopPolling,
    resume: startPolling,
    isActive: isPolling,
  } = useTimeoutPoll(syncDownloadStatus, POLL_INTERVAL, { immediate: false });

  /**
   * 同步所有活跃下载任务的状态
   */
  async function syncDownloadStatus() {
    const activeGids = await getActiveGids();

    if (activeGids.length > 0) {
      try {
        const res = await batchTellStatus(activeGids);
        const results = res.result;
        if (Array.isArray(results)) {
          for (const resultWrapper of results) {
            const task = Array.isArray(resultWrapper) ? resultWrapper[0] : resultWrapper?.result;
            if (!task?.gid) continue;

            const totalLength = Number(task.totalLength) || 0;
            const completedLength = Number(task.completedLength) || 0;
            const downloadSpeed = Number(task.downloadSpeed) || 0;

            const updates: Partial<DownLoadFile> = {
              status: task.status,
              downloadSpeed,
            };

            if (totalLength > 0) {
              updates.size = totalLength;
              updates.progress = Math.round((completedLength / totalLength) * 10000) / 100;
            }

            if (task.files?.[0]?.path) {
              updates.path = task.files[0].path;
            }

            if (downloadSpeed > 0 && totalLength > completedLength) {
              updates.eta = Math.ceil((totalLength - completedLength) / downloadSpeed);
            } else {
              updates.eta = undefined;
            }

            if (task.status === 'complete') {
              updates.completedAt = Date.now();
              updates.downloadSpeed = 0;
              await updateDownload(task.gid, updates);
              await cleanupAria2Task(task.gid);
            } else if (task.status === 'error') {
              updates.errorMessage = task.errorMessage || '下载出错';
              updates.errorCode = task.errorCode;
              updates.downloadSpeed = 0;
              await updateDownload(task.gid, updates);
              await cleanupAria2Task(task.gid);
            } else {
              await updateDownload(task.gid, updates);
            }
          }
        }
      } catch (e) {
        console.error('批量获取状态失败:', e);
      }
    }

    // 聚合文件夹状态
    await aggregateFolderStatuses();

    // 刷新 UI
    await refreshDisplayList();

    // 检查是否还需要轮询
    const stillActive = await getActiveGids();
    if (stillActive.length === 0 && !isResuming.value) {
      stopPolling();
    }
  }

  /**
   * 聚合文件夹下载状态
   */
  async function aggregateFolderStatuses() {
    const allItems = await getAllDownloads();
    const folders = allItems.filter((d) => d.isFolder && !d.isCollecting && d.status !== 'removed');

    for (const folder of folders) {
      const children = allItems.filter((d) => d.parentGid === folder.gid);
      if (children.length === 0 && folder.status !== 'complete' && folder.status !== 'error')
        continue;

      const completed = children.filter((d) => d.status === 'complete').length;
      const failed = children.filter((d) => d.status === 'error').length;
      const activeChildren = children.filter((d) => d.status === 'active');
      const paused = children.filter((d) => d.status === 'paused').length;

      const totalSize = children.reduce((sum, d) => sum + (d.size || 0), 0);
      const completedSize = children.reduce((sum, d) => {
        if (d.status === 'complete') return sum + (d.size || 0);
        if (d.progress && d.size) return sum + (d.size * d.progress) / 100;
        return sum;
      }, 0);

      const dlSpeed = activeChildren.reduce((sum, d) => sum + (d.downloadSpeed || 0), 0);

      const updates: Partial<DownLoadFile> = {
        completedFiles: completed,
        failedFiles: failed,
        size: totalSize > 0 ? totalSize : folder.size,
        progress: totalSize > 0 ? Math.round((completedSize / totalSize) * 10000) / 100 : 0,
        downloadSpeed: dlSpeed,
        eta:
          dlSpeed > 0 && totalSize > completedSize
            ? Math.ceil((totalSize - completedSize) / dlSpeed)
            : undefined,
      };

      if (completed + failed === children.length && children.length > 0) {
        if (failed > 0) {
          updates.status = 'error';
          updates.errorMessage = `${failed} 个文件下载失败`;
          updates.downloadSpeed = 0;
        } else {
          updates.status = 'complete';
          updates.completedAt = folder.completedAt ?? Date.now();
          updates.downloadSpeed = 0;
        }
      } else if (paused > 0 && activeChildren.length === 0) {
        updates.status = 'paused';
      } else {
        updates.status = 'active';
      }

      await updateDownload(folder.gid, updates);
    }
  }

  // ==================== 下载操作 ====================

  const enqueueFile = (file: MyFile, path?: string) => {
    downloadQueue.value.push({ file, path, retryCount: 0 });
    processQueue();
  };

  const enqueueFolder = async (folder: MyFile) => {
    const folderGid = `folder-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const downloadPath = settingStore.downloadSetting.downloadPath;

    const folderEntry: DownLoadFile = {
      name: folder.fn,
      fid: folder.fid,
      pickCode: folder.pc,
      size: 0,
      gid: folderGid,
      status: 'active',
      isFolder: true,
      isCollecting: true,
      totalFiles: 0,
      completedFiles: 0,
      failedFiles: 0,
      path: downloadPath ? `${downloadPath}/${folder.fn}` : undefined,
      createdAt: Date.now(),
    };
    await insertDownload(folderEntry);
    await refreshDisplayList();

    const allFiles: { file: MyFile; path: string }[] = [];
    try {
      await collectFolderFiles(folder.fid, folder.fn, allFiles);
    } catch (e) {
      console.error('收集文件夹文件失败:', e);
      await updateDownload(folderGid, {
        status: 'error',
        isCollecting: false,
        errorMessage: '获取文件列表失败',
      });
      await refreshDisplayList();
      return;
    }

    const totalSize = allFiles.reduce((sum, f) => sum + (f.file.fs || 0), 0);
    await updateDownload(folderGid, {
      totalFiles: allFiles.length,
      size: totalSize,
      isCollecting: false,
    });

    if (allFiles.length === 0) {
      await updateDownload(folderGid, { status: 'complete', completedAt: Date.now() });
      await refreshDisplayList();
      return;
    }

    for (const { file, path } of allFiles) {
      downloadQueue.value.push({ file, path, retryCount: 0, parentGid: folderGid });
    }
    await refreshDisplayList();
    processQueue();
  };

  const collectFolderFiles = async (
    folderId: string,
    currentPath: string,
    result: { file: MyFile; path: string }[],
    offset = 0,
  ) => {
    const res = await fileList({ cid: folderId, show_dir: 1, offset, limit: 1150 });

    for (const item of res.data) {
      if (item.fc === '0') {
        await sleep(FOLDER_LIST_DELAY);
        await collectFolderFiles(item.fid, `${currentPath}/${item.fn}`, result);
      } else {
        result.push({ file: item, path: currentPath });
      }
    }

    if (offset + res.data.length < res.count) {
      await sleep(FOLDER_LIST_DELAY);
      await collectFolderFiles(folderId, currentPath, result, offset + 1150);
    }
  };

  const processQueue = async () => {
    if (isProcessing.value) return;
    isProcessing.value = true;

    try {
      while (downloadQueue.value.length > 0) {
        const item = downloadQueue.value.shift()!;

        try {
          await downloadSingleFile(item);
        } catch (error) {
          if (item.retryCount < MAX_RETRY) {
            item.retryCount++;
            const isRateLimit = isRateLimitError(error);
            const delay = isRateLimit
              ? getBackoffDelay(item.retryCount)
              : getBackoffDelay(Math.max(0, item.retryCount - 1));

            console.warn(
              `下载失败${isRateLimit ? '(限流)' : ''}，${delay / 1000}s 后重试第 ${item.retryCount} 次: ${item.file.fn}`,
              error,
            );

            if (isRateLimit) {
              await sleep(delay);
            }
            downloadQueue.value.unshift(item);
          } else {
            console.error(`下载失败，已超过最大重试次数: ${item.file.fn}`, error);
            await insertDownload({
              name: item.file.fn,
              fid: item.file.fid,
              pickCode: item.file.pc,
              size: item.file.fs,
              gid: `failed-${Date.now()}-${Math.random().toString(36).slice(2)}`,
              status: 'error',
              parentGid: item.parentGid,
              errorMessage: isRateLimitError(error)
                ? '服务器限流，请稍后重试'
                : '获取下载链接失败，请稍后重试',
              createdAt: Date.now(),
            });
            await refreshDisplayList();
          }
        }

        if (downloadQueue.value.length > 0) {
          await sleep(FETCH_DELAY);
        }
      }
    } finally {
      isProcessing.value = false;
    }
  };

  const downloadSingleFile = async (item: DownloadQueueItem) => {
    const { file, path, parentGid } = item;

    const res = await fileDownloadUrl({ pick_code: file.pc });
    const fileData = res.data[file.fid];
    if (!fileData) throw new Error(`获取文件 ${file.fn} 下载信息失败`);

    const aria2res = await addUri(fileData.url.url, fileData.file_name, path);
    if (aria2res.result) {
      await insertDownload({
        name: file.fn,
        fid: file.fid,
        pickCode: file.pc,
        size: file.fs,
        gid: aria2res.result,
        status: 'active',
        parentGid,
        createdAt: Date.now(),
      });

      if (!parentGid) {
        await refreshDisplayList();
      }
      startPolling();
    }
  };

  // ==================== 公开接口 ====================

  const download = async (file: MyFile) => {
    if (file.fc === '0') {
      await enqueueFolder(file);
    } else {
      enqueueFile(file);
    }
    startPolling();
  };

  const batchDownload = async (files: MyFile[]) => {
    for (const file of files) {
      if (file.fc === '0') {
        await enqueueFolder(file);
        await sleep(FOLDER_LIST_DELAY);
      } else {
        enqueueFile(file);
      }
    }
    startPolling();
  };

  const retryDownload = async (downloadFile: DownLoadFile) => {
    if (downloadFile.isFolder) {
      await retryFolderDownload(downloadFile);
      return;
    }

    await deleteDownload(downloadFile.gid);

    try {
      const res = await fileDownloadUrl({ pick_code: downloadFile.pickCode });
      const fileData = res.data[downloadFile.fid];
      if (!fileData) throw new Error('获取下载链接失败');

      const aria2res = await addUri(fileData.url.url, fileData.file_name);
      if (aria2res.result) {
        await insertDownload({
          ...downloadFile,
          gid: aria2res.result,
          status: 'active',
          progress: 0,
          downloadSpeed: 0,
          errorMessage: undefined,
          errorCode: undefined,
          eta: undefined,
          createdAt: Date.now(),
          completedAt: undefined,
        });
        await refreshDisplayList();
        startPolling();
      }
    } catch (e) {
      console.error('重试下载失败:', e);
      throw e;
    }
  };

  const retryFolderDownload = async (folder: DownLoadFile) => {
    const children = await getChildDownloads(folder.gid);
    const failedChildren = children.filter((d) => d.status === 'error');
    if (failedChildren.length === 0) return;

    for (const child of failedChildren) {
      await deleteDownload(child.gid);
    }

    for (const child of failedChildren) {
      downloadQueue.value.push({
        file: {
          fid: child.fid,
          fn: child.name,
          pc: child.pickCode,
          fs: child.size,
          fc: '1',
        } as MyFile,
        retryCount: 0,
        parentGid: folder.gid,
      });
    }

    await updateDownload(folder.gid, {
      status: 'active',
      failedFiles: 0,
      errorMessage: undefined,
      completedAt: undefined,
    });
    await refreshDisplayList();

    processQueue();
    startPolling();
  };

  const removeTask = async (downloadFile: DownLoadFile) => {
    if (downloadFile.isFolder) {
      await removeFolderTask(downloadFile);
      return;
    }
    await removeSingleTask(downloadFile);
    await refreshDisplayList();
  };

  const removeSingleTask = async (downloadFile: DownLoadFile) => {
    try {
      if (
        downloadFile.status === 'active' ||
        downloadFile.status === 'waiting' ||
        downloadFile.status === 'paused'
      ) {
        await forceRemove(downloadFile.gid);
        await sleep(300);
        await cleanupAria2Task(downloadFile.gid);
      }
    } catch {
      // aria2 中可能已经不存在了
    }
    await deleteDownload(downloadFile.gid);
  };

  const removeFolderTask = async (folder: DownLoadFile) => {
    downloadQueue.value = downloadQueue.value.filter((q) => q.parentGid !== folder.gid);

    const children = await getChildDownloads(folder.gid);
    for (const child of children) {
      await removeSingleTask(child);
    }
    await deleteChildDownloads(folder.gid);
    await deleteDownload(folder.gid);
    await refreshDisplayList();
  };

  const clearFinished = async () => {
    await deleteFinishedDownloads();
    await refreshDisplayList();
  };

  const pauseFolder = async (folder: DownLoadFile) => {
    const children = await getChildDownloads(folder.gid);
    const active = children.filter((d) => d.status === 'active' || d.status === 'waiting');
    for (const child of active) {
      try {
        await pauseAria2(child.gid);
      } catch (e) {
        console.error('暂停子任务失败:', e);
      }
    }
  };

  const resumeFolder = async (folder: DownLoadFile) => {
    const children = await getChildDownloads(folder.gid);
    const paused = children.filter((d) => d.status === 'paused');
    for (const child of paused) {
      try {
        await unpauseAria2(child.gid);
      } catch (e) {
        console.error('恢复子任务失败:', e);
      }
    }
  };

  // ==================== 启动时恢复下载 ====================

  const resumeIncompleteDownloads = async () => {
    const allItems = await getAllDownloads();

    // 修复中断的文件夹收集
    const collecting = allItems.filter((d) => d.isFolder && d.isCollecting);
    for (const d of collecting) {
      await updateDownload(d.gid, {
        isCollecting: false,
        status: 'error',
        errorMessage: '文件收集被中断，请重试',
      });
    }

    const tasksToResume = await getIncompleteDownloads();
    if (tasksToResume.length === 0) {
      await refreshDisplayList();
      return;
    }

    console.log(`恢复 ${tasksToResume.length} 个未完成的下载任务...`);
    isResuming.value = true;
    startPolling();

    for (const task of tasksToResume) {
      try {
        const res = await fileDownloadUrl({ pick_code: task.pickCode });
        const fileData = res.data[task.fid];
        if (!fileData) {
          await updateDownload(task.gid, {
            status: 'error',
            errorMessage: '恢复失败：获取下载链接失败',
            downloadSpeed: 0,
          });
          continue;
        }

        let subPath: string | undefined;
        if (task.parentGid) {
          const allDl = await getAllDownloads();
          const parentFolder = allDl.find((d) => d.gid === task.parentGid);
          if (parentFolder?.path) {
            const basePath = settingStore.downloadSetting.downloadPath;
            if (parentFolder.path.startsWith(basePath)) {
              subPath = parentFolder.path.slice(basePath.length + 1);
            }
          }
        }

        const aria2res = await addUri(fileData.url.url, fileData.file_name, subPath);
        if (aria2res.result) {
          // gid 变了，需要删除旧记录并插入新记录
          const oldGid = task.gid;
          await deleteDownload(oldGid);
          await insertDownload({
            ...task,
            gid: aria2res.result,
            status: 'active',
            downloadSpeed: 0,
            errorMessage: undefined,
            errorCode: undefined,
          });
        }

        await sleep(RESUME_FETCH_DELAY);
      } catch (e) {
        console.error(`恢复下载失败: ${task.name}`, e);
        if (isRateLimitError(e)) {
          await sleep(getBackoffDelay(2));
        }
        await updateDownload(task.gid, {
          status: 'error',
          errorMessage: '恢复下载失败，请手动重试',
          downloadSpeed: 0,
        });
      }
    }

    isResuming.value = false;
    await refreshDisplayList();
    console.log('下载恢复完成');
  };

  // ==================== 计算属性 ====================

  const queueStatus = computed(() => ({
    queueLength: downloadQueue.value.length,
    isProcessing: isProcessing.value,
    isPolling: isPolling.value,
    isResuming: isResuming.value,
  }));

  const downloadStats = computed(() => {
    const list = displayList.value;
    const active = list.filter((d) => d.status === 'active');
    const totalSpeed = active.reduce((sum, d) => sum + (d.downloadSpeed || 0), 0);
    return {
      activeCount: active.length,
      totalSpeed,
      completed: list.filter((d) => d.status === 'complete').length,
      failed: list.filter((d) => d.status === 'error').length,
      paused: list.filter((d) => d.status === 'paused').length,
      waiting: list.filter((d) => d.status === 'waiting').length,
      total: list.length,
    };
  });

  // ==================== 初始化 ====================

  tryOnMounted(() => {
    resumeIncompleteDownloads();
  });

  return {
    displayList,
    download,
    batchDownload,
    retryDownload,
    removeTask,
    clearFinished,
    pauseFolder,
    resumeFolder,
    startPolling,
    stopPolling,
    syncDownloadStatus,
    queueStatus,
    downloadStats,
  };
});
