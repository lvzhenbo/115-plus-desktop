import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { fileDownloadUrl, fileList } from '@/api/file';
import type { MyFile } from '@/api/types/file';
import { useSettingStore } from '@/store/setting';
import { useUserStore } from '@/store/user';

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

/**
 * 下载管理器（薄封装层）
 *
 * - 接收 Rust download:* 事件驱动 displayList
 * - 所有操作通过 invoke download_* Tauri command
 * - 需在 Home 页面调用 `init()` 完成初始化
 */
export const useDownloadManager = createSharedComposable(() => {
  const settingStore = useSettingStore();

  const displayList = ref<DownLoadFile[]>([]);
  const progressCache = new Map<string, ProgressSnapshot>();
  const collectingItems = new Map<string, DownLoadFile>();
  const unlisteners: UnlistenFn[] = [];

  /** 根据 store 限速配置计算 bytes/sec */
  const computeSpeedLimitBytes = (): number => {
    if (!settingStore.downloadSetting.speedLimitEnabled) return 0;
    const value = settingStore.downloadSetting.speedLimitValue;
    const unit = settingStore.downloadSetting.speedLimitUnit;
    return unit === 'MB/s' ? value * 1024 * 1024 : value * 1024;
  };

  /** 获取通用下载参数 */
  const getDownloadParams = () => ({
    token: useUserStore().accessToken,
    userAgent: navigator.userAgent,
    split: settingStore.downloadSetting.split,
    maxGlobalConnections: settingStore.downloadSetting.maxGlobalConnections,
  });

  // ---------- download:* 事件监听 ----------

  const setupDownloadListeners = async () => {
    // download:state-sync — 完整任务列表替换（per D-02, D-05）
    unlisteners.push(
      await listen<DownLoadFile[]>('download:state-sync', (event) => {
        const pausingGids = new Set(
          displayList.value.filter((d) => d.status === 'pausing').map((d) => d.gid),
        );

        displayList.value = event.payload;

        // 恢复 pausing 前端临时状态
        for (const item of displayList.value) {
          if (pausingGids.has(item.gid) && item.status === 'active') {
            item.status = 'pausing';
          }
        }

        // progressCache 叠加：防止 DB 滞后数据导致进度回跳
        for (const item of displayList.value) {
          const snap = progressCache.get(item.gid);
          if (snap && ['active', 'pausing', 'paused'].includes(item.status ?? '')) {
            item.progress = snap.progress;
            if (item.status === 'paused') {
              item.downloadSpeed = 0;
              item.eta = undefined;
            } else {
              item.downloadSpeed = snap.speed;
              item.eta = snap.eta;
            }
            if (item.isFolder) {
              if (snap.completedFiles != null) item.completedFiles = snap.completedFiles;
              if (snap.failedFiles != null) item.failedFiles = snap.failedFiles;
              if (snap.totalFiles != null) item.totalFiles = snap.totalFiles;
            }
          }
        }

        // 合并 collectingItems 占位项
        for (const placeholder of collectingItems.values()) {
          if (!displayList.value.some((d) => d.gid === placeholder.gid)) {
            displayList.value.unshift(placeholder);
          }
        }

        // 清理 progressCache 中已不存在的 gid
        const currentGids = new Set(displayList.value.map((d) => d.gid));
        for (const gid of progressCache.keys()) {
          if (!currentGids.has(gid)) progressCache.delete(gid);
        }
      }),
    );

    // download:progress — 批量进度更新（per D-02）
    unlisteners.push(
      await listen<ProgressItem[]>('download:progress', (event) => {
        for (const pi of event.payload) {
          const progress =
            pi.totalBytes > 0
              ? Math.min(100, Math.round((pi.downloadedBytes / pi.totalBytes) * 10000) / 100)
              : 0;
          const eta = pi.etaSecs != null ? Math.ceil(pi.etaSecs) : undefined;

          progressCache.set(pi.taskId, {
            speed: pi.speed,
            progress,
            eta,
            downloadedBytes: pi.downloadedBytes,
            totalBytes: pi.totalBytes,
            completedFiles: pi.completedFiles,
            failedFiles: pi.failedFiles,
            totalFiles: pi.totalFiles,
          });

          const target = displayList.value.find((d) => d.gid === pi.taskId);
          if (target) {
            target.downloadSpeed = pi.speed;
            target.progress = progress;
            target.eta = eta;
            if (target.isFolder) {
              if (pi.completedFiles != null) target.completedFiles = pi.completedFiles;
              if (pi.failedFiles != null) target.failedFiles = pi.failedFiles;
              if (pi.totalFiles != null) target.totalFiles = pi.totalFiles;
            }
          }
        }
      }),
    );

    // download:url-needed — URL 过期自动刷新（per D-05）
    unlisteners.push(
      await listen<UrlNeededPayload>('download:url-needed', async (event) => {
        const { requestId, pickCode } = event.payload;
        try {
          const res = await fileDownloadUrl({ pick_code: pickCode });
          const fileData = Object.values(res.data)[0];
          if (fileData?.url?.url) {
            await invoke('download_provide_url', { requestId, url: fileData.url.url });
          }
        } catch (e) {
          console.error('URL 刷新失败:', e);
        }
      }),
    );

    // download:task-status — 即时单任务状态更新（弥补 state-sync 去抖延迟）
    unlisteners.push(
      await listen<DownLoadFile>('download:task-status', (event) => {
        const task = event.payload;
        const target = displayList.value.find((d) => d.gid === task.gid);
        if (!target) return;

        target.status = task.status;
        target.errorMessage = task.errorMessage;
        target.errorCode = task.errorCode;
        if (typeof task.progress === 'number') target.progress = task.progress;
        if (task.completedFiles != null) target.completedFiles = task.completedFiles;
        if (task.failedFiles != null) target.failedFiles = task.failedFiles;
        if (task.totalFiles != null) target.totalFiles = task.totalFiles;

        if (
          task.status === 'complete' ||
          task.status === 'error' ||
          task.status === 'partial_error' ||
          task.status === 'verify_failed'
        ) {
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
      }),
    );
  };

  // ---------- 递归收集文件夹 ----------

  const collectFolderFiles = async (
    folderId: string,
    currentPath: string,
    result: { file: MyFile; path: string }[],
    offset = 0,
  ) => {
    const res = await fileList({ cid: folderId, show_dir: 1, offset, limit: 1150 });

    for (const item of res.data) {
      if (item.fc === '0') {
        // 子文件夹：拼接相对路径继续递归
        const subPath = currentPath ? `${currentPath}/${item.fn}` : item.fn;
        await collectFolderFiles(item.fid, subPath, result);
      } else {
        // 文件：path 为相对于文件夹根目录的完整文件路径（含文件名）
        const filePath = currentPath ? `${currentPath}/${item.fn}` : item.fn;
        result.push({ file: item, path: filePath });
      }
    }

    if (offset + res.data.length < res.count) {
      await collectFolderFiles(folderId, currentPath, result, offset + 1150);
    }
  };

  // ---------- 公开接口 ----------

  /** 下载单个文件或文件夹 */
  const download = async (file: MyFile) => {
    const downloadPath = settingStore.downloadSetting.downloadPath;

    if (file.fc === '0') {
      // 文件夹下载 — 本地占位 + 收集 + download_enqueue_folder（per D-03）
      const parentGid = `folder-${Date.now()}-${Math.random().toString(36).slice(2)}`;
      const placeholder: DownLoadFile = {
        gid: parentGid,
        fid: file.fid,
        name: file.fn,
        pickCode: file.pc,
        size: 0,
        status: 'active',
        isFolder: true,
        isCollecting: true,
        totalFiles: 0,
        completedFiles: 0,
        failedFiles: 0,
        path: downloadPath ? `${downloadPath}/${file.fn}` : undefined,
        createdAt: Date.now(),
      };
      collectingItems.set(parentGid, placeholder);
      displayList.value.unshift(placeholder);

      const allFiles: { file: MyFile; path: string }[] = [];
      try {
        // 从空路径开始递归，path 为相对于文件夹根目录的路径（含文件名）
        await collectFolderFiles(file.fid, '', allFiles);
      } catch (e) {
        console.error('收集文件夹文件失败:', e);
        placeholder.status = 'error';
        placeholder.isCollecting = false;
        placeholder.errorMessage = '获取文件列表失败';
        return;
      }

      collectingItems.delete(parentGid);
      displayList.value = displayList.value.filter((d) => d.gid !== parentGid);

      if (allFiles.length === 0) return;

      const files: FolderFileItem[] = allFiles.map((f) => ({
        fid: f.file.fid,
        name: f.file.fn,
        pickCode: f.file.pc,
        size: f.file.fs,
        path: f.path,
      }));

      await invoke('download_enqueue_folder', {
        parentGid,
        parentName: file.fn,
        parentPath: `${downloadPath}/${file.fn}`,
        files,
        ...getDownloadParams(),
      });
    } else {
      // 单文件下载
      const res = await fileDownloadUrl({ pick_code: file.pc });
      const fileData = res.data[file.fid];
      if (!fileData) {
        console.error(`获取文件 ${file.fn} 下载信息失败`);
        return;
      }
      const savePath = `${downloadPath}/${fileData.file_name}`;
      await invoke('download_enqueue_file', {
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
      await invoke('download_retry_folder', {
        parentGid: downloadFile.gid,
        ...getDownloadParams(),
      });
    } else {
      await invoke('download_retry_task', { gid: downloadFile.gid, ...getDownloadParams() });
    }
  };

  /** 移除下载任务 */
  const removeTask = async (downloadFile: DownLoadFile) => {
    if (downloadFile.isFolder) {
      await invoke('download_cancel_folder', { parentGid: downloadFile.gid });
    } else {
      await invoke('download_cancel_task', { gid: downloadFile.gid });
    }
    // 立即从显示列表移除，不等待 state-sync 的 150ms 延迟
    displayList.value = displayList.value.filter((d) => d.gid !== downloadFile.gid);
    progressCache.delete(downloadFile.gid);
  };

  /** 清除所有已完成的下载记录 */
  const clearFinished = async () => {
    await invoke('download_delete_finished_tasks');
    // download_delete_finished_tasks 不触发 state-sync，手动刷新任务列表
    const tasks = await invoke<DownLoadFile[]>('download_get_top_level_tasks');
    displayList.value = tasks;
  };

  /** 暂停文件夹下载 */
  const pauseFolder = async (folder: DownLoadFile) => {
    const folderItem = displayList.value.find((d) => d.gid === folder.gid);
    if (folderItem) folderItem.status = 'pausing';
    await invoke('download_pause_folder', { parentGid: folder.gid });
  };

  /** 恢复文件夹下载 */
  const resumeFolder = async (folder: DownLoadFile) => {
    await invoke('download_resume_folder', { parentGid: folder.gid, ...getDownloadParams() });
  };

  /** 恢复单个已暂停的下载任务 */
  const resumeSingleFile = async (item: DownLoadFile) => {
    await invoke('download_resume_task', { gid: item.gid, ...getDownloadParams() });
  };

  /** 暂停所有活跃的下载任务 */
  const pauseAllTasks = async () => {
    for (const item of displayList.value) {
      if (item.status === 'active') {
        item.status = 'pausing';
      }
    }
    await invoke('download_pause_all');
  };

  /** 恢复所有暂停的下载任务 */
  const resumeAllTasks = async () => {
    await invoke('download_resume_all', getDownloadParams());
  };

  // ---------- computed 状态 ----------

  /** 队列状态 */
  const queueStatus = computed(() => {
    const queueLength = displayList.value.filter((d) => d.status === 'waiting').length;
    const isProcessing = displayList.value.some((d) => d.status === 'active');
    return { queueLength, isProcessing };
  });

  /** 下载统计 */
  const downloadStats = computed(() => {
    const list = displayList.value;
    const activeCount = list.filter((d) => d.status === 'active').length;
    const totalSpeed = list
      .filter((d) => d.status === 'active')
      .reduce((sum, d) => sum + (d.downloadSpeed || 0), 0);
    const completed = list.filter((d) => d.status === 'complete').length;
    const failed = list.filter((d) => d.status === 'error').length;
    const paused = list.filter((d) => d.status === 'paused' || d.status === 'pausing').length;
    const waiting = list.filter((d) => d.status === 'waiting').length;
    const total = list.length;
    return { activeCount, totalSpeed, completed, failed, paused, waiting, total };
  });

  /** 是否有活跃的下载任务（对齐旧版 db/downloads.ts hasActiveDownloads SQL 语义） */
  const hasActiveDownloads = computed(() =>
    displayList.value.some(
      (d) =>
        d.status === 'active' ||
        d.status === 'waiting' ||
        d.status === 'paused' ||
        d.status === 'pausing' ||
        d.isCollecting,
    ),
  );

  // ---------- 初始化 ----------

  /** 初始化：监听限速配置 + 建立 download:* 事件监听 */
  const init = async () => {
    await setupDownloadListeners();

    // 拉取初始任务列表（防止 recovery state-sync 在监听建立前已发出）
    const tasks = await invoke<DownLoadFile[]>('download_get_top_level_tasks');
    if (tasks.length > 0 && displayList.value.length === 0) {
      displayList.value = tasks;
    }

    // 同步初始设置到 Rust
    await invoke('download_set_max_concurrent', { n: settingStore.downloadSetting.maxConcurrent });
    await invoke('download_set_speed_limit', { bytesPerSec: computeSpeedLimitBytes() });

    // 并发数变更时通知 Rust
    watch(
      () => settingStore.downloadSetting.maxConcurrent,
      async (n) => {
        await invoke('download_set_max_concurrent', { n });
      },
    );

    // 限速配置变更时通知 Rust
    watch(
      () => [
        settingStore.downloadSetting.speedLimitEnabled,
        settingStore.downloadSetting.speedLimitValue,
        settingStore.downloadSetting.speedLimitUnit,
      ],
      async () => {
        const bytes = computeSpeedLimitBytes();
        await invoke('download_set_speed_limit', { bytesPerSec: bytes });
      },
    );
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
    resumeAllTasks,
    queueStatus,
    downloadStats,
    hasActiveDownloads,
  };
});
