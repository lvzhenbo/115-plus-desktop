import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { fileDownloadUrl, fileList } from '@/api/file';
import type { MyFile } from '@/api/types/file';
import { useSettingStore, type DownLoadFile } from '@/store/setting';
import { useUserStore } from '@/store/user';
import {
  insertDownload,
  updateDownload,
  deleteDownload,
  deleteChildDownloads,
  deleteFinishedDownloads,
  getAllDownloads,
  getActiveGids,
  getChildDownloads,
  getDownloadByGid,
  getIncompleteDownloads,
  getTopLevelDownloads,
} from '@/db/downloads';
import { sleep, isRateLimitError, getBackoffDelay } from '@/utils/rateLimit';

/** 下载队列项 */
interface DownloadQueueItem {
  file: MyFile;
  path?: string;
  retryCount: number;
  parentGid?: string;
}

/** Tauri Event: 下载进度 */
interface DownloadProgressPayload {
  task_id: string;
  downloaded_bytes: number;
  total_bytes: number;
  speed: number;
  eta_secs: number | null;
}

/** Tauri Event: 任务状态变更 */
interface DownloadTaskStatusPayload {
  task_id: string;
  status: 'Pending' | 'Active' | 'Paused' | 'Complete' | 'Error' | 'VerifyFailed';
}

/** Tauri Event: URL 过期 */
interface UrlExpiredPayload {
  task_id: string;
  pick_code: string;
}

/** Rust TaskStatus → 前端 status 映射 */
const mapRustStatus = (s: string): DownLoadFile['status'] => {
  const map: Record<string, DownLoadFile['status']> = {
    Active: 'active',
    Paused: 'paused',
    Complete: 'complete',
    Error: 'error',
    Pending: 'waiting',
    VerifyFailed: 'error',
  };
  return map[s] ?? 'waiting';
};

/**
 * 下载管理器
 *
 * - SQLite 持久化下载列表，响应式 `displayList` 驱动 UI
 * - Tauri invoke 启动下载，Event 驱动进度/状态更新
 * - 需在 Home 页面调用 `init()` 完成初始化
 */
export const useDownloadManager = createSharedComposable(() => {
  const settingStore = useSettingStore();

  const displayList = ref<DownLoadFile[]>([]);
  const downloadQueue = ref<DownloadQueueItem[]>([]);
  const isProcessing = ref(false);
  const unlisteners: UnlistenFn[] = [];

  // ---- 文件夹子任务实时进度跟踪 ----
  /** taskId → parentGid 映射 */
  const childToParentMap = new Map<string, string>();
  /** parentGid → Map<taskId, { downloaded, total, speed }> */
  const folderChildProgress = new Map<
    string,
    Map<string, { downloaded: number; total: number; speed: number }>
  >();

  /** 注册子任务到内存追踪器 */
  const registerChildTask = (taskId: string, parentGid: string, total: number) => {
    childToParentMap.set(taskId, parentGid);
    if (!folderChildProgress.has(parentGid)) {
      folderChildProgress.set(parentGid, new Map());
    }
    folderChildProgress.get(parentGid)!.set(taskId, { downloaded: 0, total, speed: 0 });
  };

  /** 根据子任务追踪器聚合文件夹的速度/进度/ETA 到 displayList */
  const updateFolderFromChildren = (parentGid: string) => {
    const folder = displayList.value.find((d) => d.gid === parentGid);
    if (!folder) return;
    const children = folderChildProgress.get(parentGid);
    if (!children || children.size === 0) return;

    let totalSpeed = 0;
    let totalDownloaded = 0;
    children.forEach((c) => {
      totalSpeed += c.speed;
      totalDownloaded += c.downloaded;
    });

    folder.downloadSpeed = totalSpeed;
    if (folder.size && folder.size > 0) {
      folder.progress = Math.min(100, Math.round((totalDownloaded / folder.size) * 10000) / 100);
      folder.eta =
        totalSpeed > 0 ? Math.ceil((folder.size - totalDownloaded) / totalSpeed) : undefined;
    }
  };

  /** 根据 store 限速配置计算 bytes/sec */
  const computeSpeedLimitBytes = (): number => {
    if (!settingStore.downloadSetting.speedLimitEnabled) return 0;
    const value = settingStore.downloadSetting.speedLimitValue;
    const unit = settingStore.downloadSetting.speedLimitUnit;
    return unit === 'MB/s' ? value * 1024 * 1024 : value * 1024;
  };

  /** 从数据库刷新顶层列表 */
  const refreshDisplayList = async () => {
    displayList.value = await getTopLevelDownloads();
  };

  // ---------- Tauri Event 监听 ----------

  /** 注册 Tauri Event 监听器 */
  const setupEventListeners = async () => {
    // 下载进度事件 — 实时更新速度、进度、ETA
    unlisteners.push(
      await listen<DownloadProgressPayload>('download:progress', async (event) => {
        const { task_id, downloaded_bytes, total_bytes, speed, eta_secs } = event.payload;
        const item = displayList.value.find((d) => d.gid === task_id);
        if (item) {
          // 顶层单文件下载 — 直接更新
          item.downloadSpeed = speed;
          item.progress =
            total_bytes > 0
              ? Math.min(100, Math.round((downloaded_bytes / total_bytes) * 10000) / 100)
              : 0;
          item.eta = eta_secs != null ? Math.ceil(eta_secs) : undefined;
          if (total_bytes > 0) item.size = total_bytes;
        } else {
          // 文件夹子任务 — 更新追踪器并聚合到父文件夹
          let parentGid = childToParentMap.get(task_id);
          if (!parentGid) {
            // 可能是恢复的下载，从数据库查找 parentGid
            const dl = await getDownloadByGid(task_id);
            if (dl?.parentGid) {
              parentGid = dl.parentGid;
              registerChildTask(task_id, parentGid, total_bytes);
            }
          }
          if (parentGid) {
            const children = folderChildProgress.get(parentGid);
            if (children) {
              children.set(task_id, { downloaded: downloaded_bytes, total: total_bytes, speed });
            }
            updateFolderFromChildren(parentGid);
          }
        }
      }),
    );

    // 任务状态事件 — 更新状态并持久化到 DB
    unlisteners.push(
      await listen<DownloadTaskStatusPayload>('download:task-status', async (event) => {
        const { task_id, status } = event.payload;
        const mappedStatus = mapRustStatus(status);
        const updates: Partial<DownLoadFile> = { status: mappedStatus };

        if (mappedStatus === 'complete') {
          updates.completedAt = Date.now();
          updates.downloadSpeed = 0;
          updates.progress = 100;
        } else if (mappedStatus === 'error') {
          updates.downloadSpeed = 0;
          updates.errorMessage =
            status === 'VerifyFailed' ? 'SHA1 校验失败，文件可能不完整' : '下载出错';
        } else if (mappedStatus === 'paused') {
          updates.downloadSpeed = 0;
        }

        // 同步更新子任务追踪器（保持内存数据与状态一致）
        const parentGid = childToParentMap.get(task_id);
        if (parentGid) {
          const children = folderChildProgress.get(parentGid);
          const child = children?.get(task_id);
          if (child) {
            child.speed = 0;
            if (mappedStatus === 'complete') {
              child.downloaded = child.total;
            }
          }
        }

        // 状态变更时将内存中的实时进度持久化到 DB，防止 refreshDisplayList 后进度丢失
        const item = displayList.value.find((d) => d.gid === task_id);
        if (item) {
          if (updates.progress == null && item.progress != null) {
            updates.progress = item.progress;
          }
          if (updates.size == null && item.size) {
            updates.size = item.size;
          }
        }

        await updateDownload(task_id, updates);
        await aggregateFolderStatuses();
        await refreshDisplayList();
      }),
    );

    // URL 过期事件 — 自动刷新 URL
    unlisteners.push(
      await listen<UrlExpiredPayload>('download:url-expired', async (event) => {
        const { task_id, pick_code } = event.payload;
        try {
          const res = await fileDownloadUrl({ pick_code });
          const fileData = Object.values(res.data)[0];
          if (fileData?.url?.url) {
            await invoke('update_download_url', {
              taskId: task_id,
              url: fileData.url.url,
            });
          }
        } catch (e) {
          console.error('URL 刷新失败:', e);
        }
      }),
    );
  };

  /** 聚合文件夹内子任务的进度、速度、完成状态 */
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
        progress:
          totalSize > 0 ? Math.min(100, Math.round((completedSize / totalSize) * 10000) / 100) : 0,
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

  // ---------- 队列与下载 ----------

  /** 将单个文件加入下载队列 */
  const enqueueFile = (file: MyFile, path?: string) => {
    downloadQueue.value.push({ file, path, retryCount: 0 });
    processQueue();
  };

  /** 将文件夹及其所有子文件加入下载队列 */
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

  /** 递归收集文件夹下所有文件 */
  const collectFolderFiles = async (
    folderId: string,
    currentPath: string,
    result: { file: MyFile; path: string }[],
    offset = 0,
  ) => {
    const res = await fileList({ cid: folderId, show_dir: 1, offset, limit: 1150 });

    for (const item of res.data) {
      if (item.fc === '0') {
        await collectFolderFiles(item.fid, `${currentPath}/${item.fn}`, result);
      } else {
        result.push({ file: item, path: currentPath });
      }
    }

    if (offset + res.data.length < res.count) {
      await collectFolderFiles(folderId, currentPath, result, offset + 1150);
    }
  };

  /** 消费下载队列，工作池模式并行下载，任务完成后立即填补空位 */
  const processQueue = async () => {
    if (isProcessing.value) return;
    isProcessing.value = true;

    const active = new Set<Promise<void>>();

    const runItem = async (item: DownloadQueueItem) => {
      try {
        await downloadSingleFile(item);
      } catch (error) {
        const maxRetry = settingStore.downloadSetting.maxRetry;
        if (item.retryCount < maxRetry) {
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
          downloadQueue.value.push(item);
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
    };

    try {
      while (downloadQueue.value.length > 0 || active.size > 0) {
        const maxConcurrent = settingStore.downloadSetting.maxConcurrent || 1;

        // 填充并行槽位（速率由全局令牌桶控制）
        while (downloadQueue.value.length > 0 && active.size < maxConcurrent) {
          const item = downloadQueue.value.shift()!;
          const task = runItem(item).finally(() => active.delete(task));
          active.add(task);
        }

        // 等待任意一个任务完成，腾出槽位
        if (active.size > 0) {
          await Promise.race(active);
        }
      }
    } finally {
      isProcessing.value = false;
    }
  };

  /** 获取下载链接并通过 Tauri 启动下载 */
  const downloadSingleFile = async (item: DownloadQueueItem) => {
    const { file, path, parentGid } = item;
    const userStore = useUserStore();

    const res = await fileDownloadUrl({ pick_code: file.pc });
    const fileData = res.data[file.fid];
    if (!fileData) throw new Error(`获取文件 ${file.fn} 下载信息失败`);

    const downloadPath = settingStore.downloadSetting.downloadPath;
    const savePath =
      downloadPath + (path ? `/${path}/${fileData.file_name}` : `/${fileData.file_name}`);

    const taskId: string = await invoke('start_download', {
      url: fileData.url.url,
      fileName: fileData.file_name,
      fileSize: file.fs,
      savePath,
      pickCode: file.pc,
      expectedSha1: fileData.sha1,
      token: userStore.accessToken,
      userAgent: navigator.userAgent,
      split: settingStore.downloadSetting.split,
      maxConnectionsPerServer: settingStore.downloadSetting.maxConnectionsPerServer,
    });

    await insertDownload({
      name: file.fn,
      fid: file.fid,
      pickCode: file.pc,
      size: file.fs,
      gid: taskId,
      status: 'active',
      parentGid,
      path: savePath,
      createdAt: Date.now(),
    });

    // 注册子任务到内存追踪器，使进度事件能实时聚合到文件夹
    if (parentGid) {
      registerChildTask(taskId, parentGid, file.fs);
    } else {
      await refreshDisplayList();
    }
  };

  // ---------- 公开接口 ----------

  /** 下载单个文件或文件夹 */
  const download = async (file: MyFile) => {
    if (file.fc === '0') {
      await enqueueFolder(file);
    } else {
      enqueueFile(file);
    }
  };

  /** 批量下载多个文件/文件夹 */
  const batchDownload = async (files: MyFile[]) => {
    for (const file of files) {
      if (file.fc === '0') {
        await enqueueFolder(file);
      } else {
        enqueueFile(file);
      }
    }
  };

  /** 重试失败的下载任务 */
  const retryDownload = async (downloadFile: DownLoadFile) => {
    if (downloadFile.isFolder) {
      await retryFolderDownload(downloadFile);
      return;
    }

    const userStore = useUserStore();
    await deleteDownload(downloadFile.gid);

    try {
      const res = await fileDownloadUrl({ pick_code: downloadFile.pickCode });
      const fileData = res.data[downloadFile.fid];
      if (!fileData) throw new Error('获取下载链接失败');

      const downloadPath = settingStore.downloadSetting.downloadPath;
      const savePath = downloadFile.path || `${downloadPath}/${fileData.file_name}`;

      const taskId: string = await invoke('start_download', {
        url: fileData.url.url,
        fileName: fileData.file_name,
        fileSize: downloadFile.size,
        savePath,
        pickCode: downloadFile.pickCode,
        expectedSha1: fileData.sha1,
        token: userStore.accessToken,
        userAgent: navigator.userAgent,
        split: settingStore.downloadSetting.split,
        maxConnectionsPerServer: settingStore.downloadSetting.maxConnectionsPerServer,
      });

      await insertDownload({
        ...downloadFile,
        gid: taskId,
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
    } catch (e) {
      console.error('重试下载失败:', e);
      throw e;
    }
  };

  /** 重试文件夹内所有失败的子任务 */
  const retryFolderDownload = async (folder: DownLoadFile) => {
    const children = await getChildDownloads(folder.gid);
    const failedChildren = children.filter((d) => d.status === 'error');
    if (failedChildren.length === 0) return;

    for (const child of failedChildren) {
      // 清理内存进度追踪器，防止旧子任务与新子任务进度双重计算导致 >100%
      childToParentMap.delete(child.gid);
      const folderChildren = folderChildProgress.get(folder.gid);
      if (folderChildren) {
        folderChildren.delete(child.gid);
      }
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
  };

  /** 移除下载任务 */
  const removeTask = async (downloadFile: DownLoadFile) => {
    if (downloadFile.isFolder) {
      await removeFolderTask(downloadFile);
      return;
    }
    await removeSingleTask(downloadFile);
    await refreshDisplayList();
  };

  /** 移除单个任务：取消下载 + 删除数据库记录 */
  const removeSingleTask = async (downloadFile: DownLoadFile) => {
    try {
      if (
        downloadFile.status === 'active' ||
        downloadFile.status === 'waiting' ||
        downloadFile.status === 'paused'
      ) {
        await invoke('cancel_download', { taskId: downloadFile.gid });
      }
    } catch {
      // 任务可能已经完成或不存在
    }
    await deleteDownload(downloadFile.gid);
  };

  /** 移除整个文件夹任务及其所有子任务 */
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

  /** 清除所有已完成的下载记录 */
  const clearFinished = async () => {
    await deleteFinishedDownloads();
    await refreshDisplayList();
  };

  /** 暂停文件夹内所有活跃子任务 */
  const pauseFolder = async (folder: DownLoadFile) => {
    const children = await getChildDownloads(folder.gid);
    const active = children.filter((d) => d.status === 'active' || d.status === 'waiting');
    for (const child of active) {
      try {
        await invoke('pause_download', { taskId: child.gid });
      } catch (e) {
        console.error('暂停子任务失败:', e);
      }
    }
  };

  /** 恢复文件夹内所有已暂停的子任务 */
  const resumeFolder = async (folder: DownLoadFile) => {
    const children = await getChildDownloads(folder.gid);

    // 将所有子任务（含已完成的）注册到内存追踪器，确保进度聚合准确
    for (const child of children) {
      if (!childToParentMap.has(child.gid)) {
        registerChildTask(child.gid, folder.gid, child.size || 0);
      }
      // 已完成的子任务标记 downloaded = total
      if (child.status === 'complete') {
        const tracker = folderChildProgress.get(folder.gid);
        const entry = tracker?.get(child.gid);
        if (entry) {
          entry.downloaded = entry.total;
          entry.speed = 0;
        }
      }
    }

    const paused = children.filter((d) => d.status === 'paused');
    const userStore = useUserStore();
    for (const child of paused) {
      try {
        const res = await fileDownloadUrl({ pick_code: child.pickCode });
        const fileData = res.data[child.fid];
        if (fileData?.url?.url) {
          await invoke('resume_download_task', {
            taskId: child.gid,
            url: fileData.url.url,
            savePath: child.path,
            token: userStore.accessToken,
            userAgent: navigator.userAgent,
            split: settingStore.downloadSetting.split,
            maxConnectionsPerServer: settingStore.downloadSetting.maxConnectionsPerServer,
          });
        }
      } catch (e) {
        console.error('恢复子任务失败:', e);
      }
    }
  };

  /** 恢复单个已暂停的下载任务（获取新 URL + resume_download_task） */
  const resumeSingleFile = async (item: DownLoadFile) => {
    const userStore = useUserStore();
    try {
      const res = await fileDownloadUrl({ pick_code: item.pickCode });
      const fileData = res.data[item.fid];
      if (fileData?.url?.url) {
        await invoke('resume_download_task', {
          taskId: item.gid,
          url: fileData.url.url,
          savePath: item.path,
          token: userStore.accessToken,
          userAgent: navigator.userAgent,
          split: settingStore.downloadSetting.split,
          maxConnectionsPerServer: settingStore.downloadSetting.maxConnectionsPerServer,
        });
      }
    } catch (e) {
      console.error('恢复下载失败:', e);
    }
  };

  // ---------- 计算属性 ----------

  const queueStatus = computed(() => ({
    queueLength: downloadQueue.value.length,
    isProcessing: isProcessing.value,
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

  // ---------- 初始化 ----------

  let initialized = false;

  /**
   * 初始化下载管理器（仅执行一次）
   *
   * 注册 Tauri Event 监听器，将未完成任务标记为暂停。
   */
  const init = async () => {
    if (initialized) return;
    initialized = true;

    // 注册 Tauri Event 监听器
    await setupEventListeners();

    // 将未完成的任务标记为暂停
    const incompleteTasks = await getIncompleteDownloads();
    for (const task of incompleteTasks) {
      await updateDownload(task.gid, { status: 'paused', downloadSpeed: 0 });
    }
    await refreshDisplayList();

    // 初始化限速设置
    const speedBytes = computeSpeedLimitBytes();
    if (speedBytes > 0) {
      await invoke('set_speed_limit', { limit: speedBytes });
    }

    // 监听限速设置变化，实时生效
    watch(
      () => [
        settingStore.downloadSetting.speedLimitEnabled,
        settingStore.downloadSetting.speedLimitValue,
        settingStore.downloadSetting.speedLimitUnit,
      ],
      async () => {
        const limit = computeSpeedLimitBytes();
        await invoke('set_speed_limit', { limit });
      },
    );
  };

  /** 暂停所有活跃的下载任务 */
  const pauseAllTasks = async () => {
    const activeGids = await getActiveGids();
    for (const gid of activeGids) {
      try {
        await invoke('pause_download', { taskId: gid });
      } catch {
        // 任务可能已经完成或不存在
      }
    }
  };

  return {
    init,
    displayList,
    download,
    batchDownload,
    retryDownload,
    removeTask,
    clearFinished,
    pauseFolder,
    resumeFolder,
    resumeSingleFile,
    pauseAllTasks,
    queueStatus,
    downloadStats,
  };
});
