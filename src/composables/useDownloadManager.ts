import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { fileDownloadUrl, fileList } from '@/api/file';
import type { MyFile } from '@/api/types/file';
import { useSettingStore } from '@/store/setting';
import { useUserStore } from '@/store/user';

// 下载列表前端桥接层。
//
// 这里和上传管理器一样，只负责：
// - 订阅 Rust 发出的 download:* 事件
// - 把用户操作转成 download_* command
// - 处理仍必须由前端请求的直链刷新与文件夹预收集流程

export interface DownLoadFile {
  fid: string;
  name: string;
  gid: string;
  size: number;
  pickCode: string;
  status?:
    | 'active'
    | 'waiting'
    | 'paused'
    | 'pausing'
    | 'complete'
    | 'error'
    | 'partial_error'
    | 'verify_failed'
    | 'removed';
  progress?: number;
  path?: string;
  downloadSpeed?: number;
  /** 预计剩余时间 (秒) */
  eta?: number;
  /** 错误信息 */
  errorMessage?: string;
  /** 错误码 */
  errorCode?: string;
  /** 创建时间戳 */
  createdAt?: number;
  /** 完成时间戳 */
  completedAt?: number;
  /** 是否为文件夹下载任务 */
  isFolder?: boolean;
  /** 文件夹正在收集文件列表 */
  isCollecting?: boolean;
  /** 父文件夹任务的 gid（标记为子文件） */
  parentGid?: string;
  /** 文件夹内总文件数 */
  totalFiles?: number;
  /** 文件夹内已完成文件数 */
  completedFiles?: number;
  /** 文件夹内失败文件数 */
  failedFiles?: number;
}

/** download:progress 事件的单项进度快照 (camelCase, 来自 Rust ProgressItem) */
interface ProgressItem {
  taskId: string;
  downloadedBytes: number;
  totalBytes: number;
  speed: number;
  etaSecs?: number;
  status: string;
  name: string;
  isFolder?: boolean;
  completedFiles?: number;
  failedFiles?: number;
  totalFiles?: number;
}

/** download:url-needed 事件 payload (camelCase, 来自 Rust UrlNeededPayload) */
interface UrlNeededPayload {
  requestId: string;
  taskId: string;
  pickCode: string;
}

/** download_enqueue_folder 的文件项参数 */
interface FolderFileItem {
  fid: string;
  name: string;
  pickCode: string;
  size: number;
  path: string;
}

interface FolderDownloadTarget {
  gid?: string;
  fid: string;
  name: string;
  pickCode: string;
}

/** progressCache 中的快照（per D-02，防止 state-sync 进度回跳） */
interface ProgressSnapshot {
  speed: number;
  progress: number;
  eta?: number;
  downloadedBytes: number;
  totalBytes: number;
  completedFiles?: number;
  failedFiles?: number;
  totalFiles?: number;
}

export type DownloadStatus = NonNullable<DownLoadFile['status']>;

interface DownloadQueueStatus {
  queueLength: number;
  isProcessing: boolean;
}

interface DownloadStats {
  activeCount: number;
  totalSpeed: number;
  completed: number;
  failed: number;
  paused: number;
  waiting: number;
  total: number;
}

const ACTIVE_DOWNLOAD_STATUS_SET = new Set<DownloadStatus>(['active']);
const PROCESSING_DOWNLOAD_STATUS_SET = new Set<DownloadStatus>(['active']);
const PAUSED_DOWNLOAD_STATUS_SET = new Set<DownloadStatus>(['paused', 'pausing']);
const TERMINAL_DOWNLOAD_STATUS_SET = new Set<DownloadStatus>([
  'complete',
  'error',
  'partial_error',
  'verify_failed',
]);
const PRESERVED_DOWNLOAD_STATUS_SET = new Set<DownloadStatus>(['active', 'pausing', 'paused']);
const ACTIVE_PRESENCE_DOWNLOAD_STATUS_SET = new Set<DownloadStatus>([
  'active',
  'waiting',
  'paused',
  'pausing',
]);
const FOLDER_COLLECTION_ABORTED = 'folder-collection-aborted';

/** 统一的下载 command 调用入口。 */
const invokeDownloadCommand = async <T = void>(
  command: string,
  payload?: Record<string, unknown>,
): Promise<T> => {
  return payload ? invoke<T>(command, payload) : invoke<T>(command);
};

const isFolderCollectionAbortedError = (error: unknown) =>
  error instanceof Error && error.message === FOLDER_COLLECTION_ABORTED;

const logDownloadManagerError = (message: string, error: unknown) => {
  console.error(message, error);
};

/**
 * 下载管理器（薄封装层）
 *
 * - 接收 Rust download:* 事件驱动 displayList
 * - 所有操作通过 invoke download_* Tauri command
 * - 需在 Home 页面调用 `init()` 完成初始化
 */
export const useDownloadManager = createSharedComposable(() => {
  const settingStore = useSettingStore();
  const userStore = useUserStore();

  // displayList 是下载列表 UI 的唯一真相来源；progressCache 只用于填补 state-sync 间隙。
  const displayList = ref<DownLoadFile[]>([]);
  const progressCache = new Map<string, ProgressSnapshot>();
  const cancelledFolderCollections = new Set<string>();
  const unlisteners: UnlistenFn[] = [];
  const stopHandles: Array<() => void> = [];

  let listenerPromise: Promise<void> | null = null;
  let initPromise: Promise<void> | null = null;

  /** 根据 store 限速配置计算 bytes/sec */
  const computeSpeedLimitBytes = (): number => {
    if (!settingStore.downloadSetting.speedLimitEnabled) return 0;
    const value = settingStore.downloadSetting.speedLimitValue;
    const unit = settingStore.downloadSetting.speedLimitUnit;
    return unit === 'MB/s' ? value * 1024 * 1024 : value * 1024;
  };

  /** 获取通用下载参数 */
  const getDownloadParams = () => ({
    token: userStore.accessToken,
    userAgent: navigator.userAgent,
    split: settingStore.downloadSetting.split,
    maxGlobalConnections: settingStore.downloadSetting.maxGlobalConnections,
  });

  const refreshDisplayList = async () => {
    const tasks = await invokeDownloadCommand<DownLoadFile[]>('download_get_top_level_tasks');
    handleStateSync(tasks);
  };

  // 运行时下载参数由用户 token、UA 和设置项共同决定。
  const syncMaxConcurrent = async (n = settingStore.downloadSetting.maxConcurrent) => {
    await invokeDownloadCommand('download_set_max_concurrent', { n });
  };

  const syncSpeedLimit = async () => {
    await invokeDownloadCommand('download_set_speed_limit', {
      bytesPerSec: computeSpeedLimitBytes(),
    });
  };

  const syncDownloadSettings = async () => {
    await Promise.all([syncMaxConcurrent(), syncSpeedLimit()]);
  };

  const updateTask = (gid: string, updater: (task: DownLoadFile) => void) => {
    const target = displayList.value.find((item) => item.gid === gid);
    if (!target) return;
    updater(target);
  };

  // 当 state-sync 回来的任务仍处于活跃/暂停态时，用最近一次进度快照补齐 UI，避免“进度倒退”。
  const applyProgressSnapshot = (task: DownLoadFile, snapshot: ProgressSnapshot) => {
    task.progress = snapshot.progress;

    if (task.status === 'paused') {
      task.downloadSpeed = 0;
      task.eta = undefined;
    } else {
      task.downloadSpeed = snapshot.speed;
      task.eta = snapshot.eta;
    }

    if (task.isFolder) {
      if (snapshot.completedFiles != null) task.completedFiles = snapshot.completedFiles;
      if (snapshot.failedFiles != null) task.failedFiles = snapshot.failedFiles;
      if (snapshot.totalFiles != null) task.totalFiles = snapshot.totalFiles;
    }
  };

  // 清理已经不在列表里的缓存快照，避免长时间运行后 map 无界增长。
  const pruneProgressCache = (tasks: DownLoadFile[]) => {
    const currentGids = new Set(tasks.map((item) => item.gid));
    for (const gid of progressCache.keys()) {
      if (!currentGids.has(gid)) {
        progressCache.delete(gid);
      }
    }
  };

  // `download:state-sync` 是顶层任务列表的权威同步事件。
  const handleStateSync = (tasks: DownLoadFile[]) => {
    const pausingGids = new Set(
      displayList.value.filter((item) => item.status === 'pausing').map((item) => item.gid),
    );

    displayList.value = tasks;

    for (const item of displayList.value) {
      if (pausingGids.has(item.gid) && item.status === 'active') {
        item.status = 'pausing';
      }

      const snapshot = progressCache.get(item.gid);
      if (snapshot && item.status && PRESERVED_DOWNLOAD_STATUS_SET.has(item.status)) {
        applyProgressSnapshot(item, snapshot);
      }
    }

    pruneProgressCache(displayList.value);
  };

  // `download:progress` 只提供瞬时指标，真正的结构化列表仍然以 state-sync 为准。
  const handleProgress = (items: ProgressItem[]) => {
    for (const item of items) {
      const progress =
        item.totalBytes > 0
          ? Math.min(100, Math.round((item.downloadedBytes / item.totalBytes) * 10000) / 100)
          : 0;
      const eta = item.etaSecs != null ? Math.ceil(item.etaSecs) : undefined;

      progressCache.set(item.taskId, {
        speed: item.speed,
        progress,
        eta,
        downloadedBytes: item.downloadedBytes,
        totalBytes: item.totalBytes,
        completedFiles: item.completedFiles,
        failedFiles: item.failedFiles,
        totalFiles: item.totalFiles,
      });

      updateTask(item.taskId, (task) => {
        task.downloadSpeed = item.speed;
        task.progress = progress;
        task.eta = eta;
        if (task.isFolder) {
          if (item.completedFiles != null) task.completedFiles = item.completedFiles;
          if (item.failedFiles != null) task.failedFiles = item.failedFiles;
          if (item.totalFiles != null) task.totalFiles = item.totalFiles;
        }
      });
    }
  };

  // `download:task-status` 负责补齐 error/complete 等终态信息。
  const handleTaskStatus = (task: DownLoadFile) => {
    updateTask(task.gid, (target) => {
      target.status = task.status;
      target.errorMessage = task.errorMessage;
      target.errorCode = task.errorCode;

      if (typeof task.progress === 'number') target.progress = task.progress;
      if (task.completedFiles != null) target.completedFiles = task.completedFiles;
      if (task.failedFiles != null) target.failedFiles = task.failedFiles;
      if (task.totalFiles != null) target.totalFiles = task.totalFiles;

      if (task.status && TERMINAL_DOWNLOAD_STATUS_SET.has(task.status)) {
        progressCache.delete(task.gid);
        target.downloadSpeed = 0;
        target.eta = undefined;
      }

      if (task.status === 'paused') {
        target.downloadSpeed = 0;
        target.eta = undefined;
      }

      if (task.status === 'complete') {
        target.progress = 100;
        target.completedAt = task.completedAt ?? Date.now();
      }
    });
  };

  // 直链刷新仍然必须由前端发起，因为它依赖现有 Web API 和鉴权上下文。
  const handleUrlNeeded = async ({ requestId, pickCode }: UrlNeededPayload) => {
    try {
      const response = await fileDownloadUrl({ pick_code: pickCode });
      const fileData = Object.values(response.data)[0];
      if (fileData?.url?.url) {
        await invokeDownloadCommand('download_provide_url', {
          requestId,
          url: fileData.url.url,
        });
      }
    } catch (error) {
      logDownloadManagerError('URL 刷新失败:', error);
    }
  };

  // ---------- download:* 事件监听 ----------

  const setupDownloadListeners = async () => {
    if (!listenerPromise) {
      listenerPromise = Promise.all([
        listen<DownLoadFile[]>('download:state-sync', (event) => {
          handleStateSync(event.payload);
        }),
        listen<ProgressItem[]>('download:progress', (event) => {
          handleProgress(event.payload);
        }),
        listen<UrlNeededPayload>('download:url-needed', (event) => {
          void handleUrlNeeded(event.payload);
        }),
        listen<DownLoadFile>('download:task-status', (event) => {
          handleTaskStatus(event.payload);
        }),
      ])
        .then((listeners) => {
          unlisteners.push(...listeners);
        })
        .catch((error) => {
          listenerPromise = null;
          throw error;
        });
    }

    await listenerPromise;
  };

  // ---------- 递归收集文件夹 ----------

  // 文件夹下载目前仍在前端先递归拿完文件列表，再一次性把结果交给 Rust 队列。
  const collectFolderFiles = async (
    folderId: string,
    currentPath: string,
    result: { file: MyFile; path: string }[],
    parentGid: string,
    offset = 0,
  ) => {
    if (cancelledFolderCollections.has(parentGid)) {
      throw new Error(FOLDER_COLLECTION_ABORTED);
    }

    const res = await fileList({ cid: folderId, show_dir: 1, offset, limit: 1150 });

    if (cancelledFolderCollections.has(parentGid)) {
      throw new Error(FOLDER_COLLECTION_ABORTED);
    }

    for (const item of res.data) {
      if (item.fc === '0') {
        // 子文件夹：拼接相对路径继续递归
        const subPath = currentPath ? `${currentPath}/${item.fn}` : item.fn;
        await collectFolderFiles(item.fid, subPath, result, parentGid);
      } else {
        // 文件：path 为相对于文件夹根目录的完整文件路径（含文件名）
        const filePath = currentPath ? `${currentPath}/${item.fn}` : item.fn;
        result.push({ file: item, path: filePath });
      }
    }

    if (offset + res.data.length < res.count) {
      await collectFolderFiles(folderId, currentPath, result, parentGid, offset + 1150);
    }
  };

  // 文件夹预收集失败时，把父任务切回 error，避免界面长期停留在 collecting 状态。
  const markFolderCollectionFailed = async (parentGid: string, errorMessage: string) => {
    try {
      await invokeDownloadCommand('download_fail_folder_collection', { parentGid, errorMessage });
    } catch (error) {
      logDownloadManagerError('更新文件夹收集失败状态失败:', error);
    }
  };

  const isFolderCollectionRetry = (item: DownLoadFile) =>
    item.isFolder &&
    !item.isCollecting &&
    (item.totalFiles ?? 0) === 0 &&
    (item.completedFiles ?? 0) === 0 &&
    (item.failedFiles ?? 0) === 0;

  // 文件夹下载的真实入口：先收集文件列表，再统一交给 Rust 建立父/子任务。
  const enqueueCollectedFolder = async (
    folder: FolderDownloadTarget,
    reuseExistingTask = false,
  ) => {
    const parentGid = folder.gid ?? `folder-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const parentPath = `${settingStore.downloadSetting.downloadPath}/${folder.name}`;

    cancelledFolderCollections.delete(parentGid);

    if (reuseExistingTask) {
      await invokeDownloadCommand('download_restart_folder_collection', { parentGid });
    } else {
      await invokeDownloadCommand('download_create_folder_task', {
        parentGid,
        parentFid: folder.fid,
        parentName: folder.name,
        parentPickCode: folder.pickCode,
        parentPath,
      });
    }

    const allFiles: { file: MyFile; path: string }[] = [];
    try {
      await collectFolderFiles(folder.fid, '', allFiles, parentGid);
    } catch (error) {
      if (isFolderCollectionAbortedError(error)) {
        cancelledFolderCollections.delete(parentGid);
        return;
      }

      logDownloadManagerError('收集文件夹文件失败:', error);
      await markFolderCollectionFailed(parentGid, '获取文件列表失败');
      throw error instanceof Error ? error : new Error('获取文件列表失败');
    }

    if (cancelledFolderCollections.delete(parentGid)) {
      return;
    }

    const files: FolderFileItem[] = allFiles.map((f) => ({
      fid: f.file.fid,
      name: f.file.fn,
      pickCode: f.file.pc,
      size: f.file.fs,
      path: f.path,
    }));

    try {
      await invokeDownloadCommand('download_enqueue_folder', {
        parentGid,
        parentFid: folder.fid,
        parentName: folder.name,
        parentPickCode: folder.pickCode,
        parentPath,
        files,
        ...getDownloadParams(),
      });
    } catch (error) {
      logDownloadManagerError('创建文件夹下载任务失败:', error);
      await markFolderCollectionFailed(parentGid, '创建下载任务失败');
      throw error instanceof Error ? error : new Error('创建下载任务失败');
    }
  };

  // ---------- 公开接口 ----------

  /** 下载单个文件或文件夹 */
  const download = async (file: MyFile) => {
    const downloadPath = settingStore.downloadSetting.downloadPath;

    if (file.fc === '0') {
      await enqueueCollectedFolder({
        fid: file.fid,
        name: file.fn,
        pickCode: file.pc,
      });
    } else {
      // 单文件下载
      const res = await fileDownloadUrl({ pick_code: file.pc });
      const fileData = res.data[file.fid];
      if (!fileData) {
        logDownloadManagerError(`获取文件 ${file.fn} 下载信息失败`, res.data);
        return;
      }
      const savePath = `${downloadPath}/${fileData.file_name}`;
      await invokeDownloadCommand('download_enqueue_file', {
        fid: file.fid,
        name: file.fn,
        pickCode: file.pc,
        size: file.fs,
        savePath,
        expectedSha1: fileData.sha1,
        ...getDownloadParams(),
      });
    }
  };

  /** 批量下载多个文件/文件夹 */
  const batchDownload = async (files: MyFile[]) => {
    for (const file of files) {
      await download(file);
    }
  };

  /** 重试失败的下载任务 */
  const retryDownload = async (downloadFile: DownLoadFile) => {
    if (downloadFile.isFolder) {
      if (isFolderCollectionRetry(downloadFile)) {
        await enqueueCollectedFolder(
          {
            gid: downloadFile.gid,
            fid: downloadFile.fid,
            name: downloadFile.name,
            pickCode: downloadFile.pickCode,
          },
          true,
        );
      } else {
        await invokeDownloadCommand('download_retry_folder', {
          parentGid: downloadFile.gid,
          ...getDownloadParams(),
        });
      }
    } else {
      await invokeDownloadCommand('download_retry_task', {
        gid: downloadFile.gid,
        ...getDownloadParams(),
      });
    }
  };

  /** 移除下载任务 */
  const removeTask = async (downloadFile: DownLoadFile) => {
    if (downloadFile.isFolder) {
      if (downloadFile.isCollecting) {
        cancelledFolderCollections.add(downloadFile.gid);
      }
      await invokeDownloadCommand('download_cancel_folder', { parentGid: downloadFile.gid });
    } else {
      await invokeDownloadCommand('download_cancel_task', { gid: downloadFile.gid });
    }
    // 立即从显示列表移除，不等待 state-sync 的 150ms 延迟
    displayList.value = displayList.value.filter((d) => d.gid !== downloadFile.gid);
    progressCache.delete(downloadFile.gid);
  };

  /** 清除所有已完成的下载记录 */
  const clearFinished = async () => {
    await invokeDownloadCommand('download_delete_finished_tasks');
    await refreshDisplayList();
  };

  /** 暂停文件夹下载 */
  const pauseFolder = async (folder: DownLoadFile) => {
    if (folder.isCollecting) return;
    const folderItem = displayList.value.find((d) => d.gid === folder.gid);
    if (folderItem) folderItem.status = 'pausing';
    await invokeDownloadCommand('download_pause_folder', { parentGid: folder.gid });
  };

  /** 恢复文件夹下载 */
  const resumeFolder = async (folder: DownLoadFile) => {
    await invokeDownloadCommand('download_resume_folder', {
      parentGid: folder.gid,
      ...getDownloadParams(),
    });
  };

  /** 恢复单个已暂停的下载任务 */
  const resumeSingleFile = async (item: DownLoadFile) => {
    await invokeDownloadCommand('download_resume_task', { gid: item.gid, ...getDownloadParams() });
  };

  /** 暂停所有活跃的下载任务 */
  const pauseAllTasks = async () => {
    for (const item of displayList.value) {
      if (item.status === 'active') {
        item.status = 'pausing';
      }
    }
    await invokeDownloadCommand('download_pause_all');
  };

  /** 恢复所有暂停的下载任务 */
  const resumeAllTasks = async () => {
    await invokeDownloadCommand('download_resume_all', getDownloadParams());
  };

  // ---------- computed 状态 ----------

  /** 队列状态 */
  const queueStatus = computed<DownloadQueueStatus>(() => {
    const queueLength = displayList.value.filter((d) => d.status === 'waiting').length;
    const isProcessing = displayList.value.some(
      (d) => d.status != null && PROCESSING_DOWNLOAD_STATUS_SET.has(d.status),
    );
    return { queueLength, isProcessing };
  });

  /** 下载统计 */
  const downloadStats = computed<DownloadStats>(() => {
    const list = displayList.value;
    const activeCount = list.filter(
      (d) => d.status != null && ACTIVE_DOWNLOAD_STATUS_SET.has(d.status),
    ).length;
    const totalSpeed = list
      .filter((d) => d.status != null && ACTIVE_DOWNLOAD_STATUS_SET.has(d.status))
      .reduce((sum, d) => sum + (d.downloadSpeed || 0), 0);
    const completed = list.filter((d) => d.status === 'complete').length;
    const failed = list.filter((d) => d.status === 'error').length;
    const paused = list.filter(
      (d) => d.status != null && PAUSED_DOWNLOAD_STATUS_SET.has(d.status),
    ).length;
    const waiting = list.filter((d) => d.status === 'waiting').length;
    const total = list.length;
    return { activeCount, totalSpeed, completed, failed, paused, waiting, total };
  });

  /** 是否有活跃的下载任务（对齐旧版 db/downloads.ts hasActiveDownloads SQL 语义） */
  const hasActiveDownloads = computed(() =>
    displayList.value.some(
      (d) =>
        (d.status != null && ACTIVE_PRESENCE_DOWNLOAD_STATUS_SET.has(d.status)) || d.isCollecting,
    ),
  );

  const setupSettingSync = () => {
    if (stopHandles.length > 0) {
      return;
    }

    stopHandles.push(
      watch(
        () => settingStore.downloadSetting.maxConcurrent,
        (n) => {
          void syncMaxConcurrent(n).catch((error) => {
            logDownloadManagerError('同步下载并发设置失败:', error);
          });
        },
      ),
      watch(
        () => [
          settingStore.downloadSetting.speedLimitEnabled,
          settingStore.downloadSetting.speedLimitValue,
          settingStore.downloadSetting.speedLimitUnit,
        ],
        () => {
          void syncSpeedLimit().catch((error) => {
            logDownloadManagerError('同步下载限速设置失败:', error);
          });
        },
      ),
    );
  };

  // shared composable 在最后一个作用域释放时统一拆除事件和 watcher。
  const dispose = () => {
    listenerPromise = null;
    initPromise = null;
    progressCache.clear();
    cancelledFolderCollections.clear();

    for (const unlisten of unlisteners.splice(0)) {
      unlisten();
    }

    for (const stop of stopHandles.splice(0)) {
      stop();
    }
  };

  tryOnScopeDispose(dispose);

  // ---------- 初始化 ----------

  /** 初始化：监听限速配置 + 建立 download:* 事件监听 */
  const init = async () => {
    if (!initPromise) {
      initPromise = (async () => {
        await setupDownloadListeners();

        if (displayList.value.length === 0) {
          await refreshDisplayList();
        }

        await syncDownloadSettings();
        setupSettingSync();
      })().catch((error) => {
        initPromise = null;
        throw error;
      });
    }

    await initPromise;
  };

  return {
    init,
    displayList: computed(() => displayList.value),
    download,
    batchDownload,
    retryDownload,
    removeTask,
    clearFinished,
    pauseFolder,
    resumeFolder,
    resumeSingleFile,
    pauseAllTasks,
    resumeAllTasks,
    queueStatus,
    downloadStats,
    hasActiveDownloads,
  };
});
