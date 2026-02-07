import { addUri, batchTellStatus, forceRemove, removeDownloadResult } from '@/api/aria2';
import { fileDownloadUrl, fileList } from '@/api/file';
import type { MyFile } from '@/api/types/file';
import { useSettingStore, type DownLoadFile } from '@/store/setting';

/** 下载队列项 */
interface DownloadQueueItem {
  file: MyFile;
  path?: string;
  retryCount: number;
}

/** 最大并发下载链接获取数 */
const MAX_CONCURRENT_FETCH = 3;
/** 最大重试次数 */
const MAX_RETRY = 3;
/** 获取下载链接之间的最小延迟(ms) */
const FETCH_DELAY = 500;
/** 状态轮询间隔(ms) */
const POLL_INTERVAL = 2000;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * 下载管理器 - 使用 createSharedComposable 确保多组件共享同一实例
 * 使用 useTimeoutPoll 替代手动 setInterval，等待异步回调完成后再调度下一次轮询
 */
export const useDownloadManager = createSharedComposable(() => {
  const settingStore = useSettingStore();

  /** 下载队列 */
  const downloadQueue = ref<DownloadQueueItem[]>([]);
  const isProcessing = ref(false);

  /**
   * 状态轮询 - 使用 useTimeoutPoll 确保上一次轮询完成后再开始下一次，防止请求重叠
   */
  const {
    pause: stopPolling,
    resume: startPolling,
    isActive: isPolling,
  } = useTimeoutPoll(syncDownloadStatus, POLL_INTERVAL, { immediate: false });

  /**
   * 同步所有活跃下载任务的状态（使用 function 声明以支持提升，供 useTimeoutPoll 引用）
   */
  async function syncDownloadStatus() {
    const list = settingStore.downloadSetting.downloadList;
    // 只查询活跃/等待/暂停状态的任务
    const activeGids = list
      .filter(
        (item) =>
          !item.status ||
          item.status === 'active' ||
          item.status === 'waiting' ||
          item.status === 'paused',
      )
      .map((item) => item.gid);

    if (activeGids.length === 0) {
      stopPolling();
      return;
    }

    try {
      const res = await batchTellStatus(activeGids);
      // system.multicall 返回数组的数组
      const results = res.result;
      if (!Array.isArray(results)) return;

      for (const resultWrapper of results) {
        // multicall 每个结果是 [result] 数组
        const task = Array.isArray(resultWrapper) ? resultWrapper[0] : resultWrapper?.result;
        if (!task?.gid) continue;

        const item = list.find((d) => d.gid === task.gid);
        if (!item) continue;

        const totalLength = Number(task.totalLength) || 0;
        const completedLength = Number(task.completedLength) || 0;
        const downloadSpeed = Number(task.downloadSpeed) || 0;

        item.status = task.status;
        item.downloadSpeed = downloadSpeed;
        item.size = totalLength || item.size;

        if (totalLength > 0) {
          item.progress = Math.round((completedLength / totalLength) * 10000) / 100;
        }

        // 记录文件路径
        if (task.files?.[0]?.path && !item.path) {
          item.path = task.files[0].path;
        }

        // 计算预计剩余时间
        if (downloadSpeed > 0 && totalLength > completedLength) {
          item.eta = Math.ceil((totalLength - completedLength) / downloadSpeed);
        } else {
          item.eta = undefined;
        }

        // 记录完成时间
        if (task.status === 'complete' && !item.completedAt) {
          item.completedAt = Date.now();
        }

        // 记录错误信息
        if (task.status === 'error') {
          item.errorMessage = task.errorMessage || '下载出错';
          item.errorCode = task.errorCode;
        }
      }
    } catch (e) {
      console.error('批量获取状态失败:', e);
    }
  }

  /**
   * 添加单个文件到下载队列
   */
  const enqueueFile = (file: MyFile, path?: string) => {
    downloadQueue.value.push({ file, path, retryCount: 0 });
    processQueue();
  };

  /**
   * 添加文件夹到下载队列 - 递归获取所有文件
   */
  const enqueueFolder = async (folder: MyFile) => {
    const allFiles: { file: MyFile; path: string }[] = [];
    await collectFolderFiles(folder.fid, folder.fn, allFiles);

    for (const { file, path } of allFiles) {
      downloadQueue.value.push({ file, path, retryCount: 0 });
    }
    processQueue();
  };

  /**
   * 递归收集文件夹下所有文件及其相对路径
   */
  const collectFolderFiles = async (
    folderId: string,
    currentPath: string,
    result: { file: MyFile; path: string }[],
    offset = 0,
  ) => {
    const res = await fileList({
      cid: folderId,
      show_dir: 1,
      offset,
      limit: 1150,
    });

    for (const item of res.data) {
      if (item.fc === '0') {
        // 子文件夹 - 递归处理
        await collectFolderFiles(item.fid, `${currentPath}/${item.fn}`, result);
      } else {
        // 文件 - 添加到结果
        result.push({ file: item, path: currentPath });
      }
    }

    // 如果还有更多文件，继续分页获取
    if (offset + res.data.length < res.count) {
      await collectFolderFiles(folderId, currentPath, result, offset + 1150);
    }
  };

  /**
   * 处理下载队列 - 控制并发
   */
  const processQueue = async () => {
    if (isProcessing.value) return;
    isProcessing.value = true;

    try {
      while (downloadQueue.value.length > 0) {
        // 取出一批任务并发处理
        const batch = downloadQueue.value.splice(0, MAX_CONCURRENT_FETCH);
        const results = await Promise.allSettled(
          batch.map(async (item, index) => {
            // 错开请求避免触发限流
            if (index > 0) await sleep(FETCH_DELAY * index);
            return downloadSingleFile(item);
          }),
        );

        // 处理失败的任务 - 添加回队列重试
        results.forEach((result, index) => {
          if (result.status === 'rejected') {
            const item = batch[index]!;
            if (item.retryCount < MAX_RETRY) {
              item.retryCount++;
              console.warn(
                `下载失败，重试第 ${item.retryCount} 次: ${item.file.fn}`,
                result.reason,
              );
              downloadQueue.value.push(item);
            } else {
              console.error(`下载失败，已超过最大重试次数: ${item.file.fn}`, result.reason);
              // 添加失败记录到列表
              settingStore.downloadSetting.downloadList.unshift({
                name: item.file.fn,
                fid: item.file.fid,
                pickCode: item.file.pc,
                size: item.file.fs,
                gid: `failed-${Date.now()}-${Math.random().toString(36).slice(2)}`,
                status: 'error',
                errorMessage: '获取下载链接失败，请稍后重试',
                createdAt: Date.now(),
              });
            }
          }
        });

        // 队列中还有任务时稍作延迟
        if (downloadQueue.value.length > 0) {
          await sleep(FETCH_DELAY);
        }
      }
    } finally {
      isProcessing.value = false;
    }
  };

  /**
   * 下载单个文件
   */
  const downloadSingleFile = async (item: DownloadQueueItem) => {
    const { file, path } = item;

    const res = await fileDownloadUrl({ pick_code: file.pc });
    const fileData = res.data[file.fid];
    if (!fileData) {
      throw new Error(`获取文件 ${file.fn} 下载信息失败`);
    }

    const aria2res = await addUri(fileData.url.url, fileData.file_name, path);
    if (aria2res.result) {
      settingStore.downloadSetting.downloadList.unshift({
        name: file.fn,
        fid: file.fid,
        pickCode: file.pc,
        size: file.fs,
        gid: aria2res.result,
        status: 'active',
        createdAt: Date.now(),
      });

      // 确保轮询已启动
      startPolling();
    }
  };

  /**
   * 智能下载入口 - 自动判断文件/文件夹
   */
  const download = async (file: MyFile) => {
    if (file.fc === '0') {
      // 文件夹
      await enqueueFolder(file);
    } else {
      enqueueFile(file);
    }
    startPolling();
  };

  /**
   * 批量下载
   */
  const batchDownload = async (files: MyFile[]) => {
    for (const file of files) {
      await download(file);
    }
  };

  /**
   * 重试失败的下载
   */
  const retryDownload = async (downloadFile: DownLoadFile) => {
    // 从列表中移除旧记录
    const index = settingStore.downloadSetting.downloadList.findIndex(
      (d) => d.gid === downloadFile.gid,
    );
    if (index !== -1) {
      settingStore.downloadSetting.downloadList.splice(index, 1);
    }

    // 重新获取下载链接
    try {
      const res = await fileDownloadUrl({ pick_code: downloadFile.pickCode });
      const fileData = res.data[downloadFile.fid];
      if (!fileData) {
        throw new Error('获取下载链接失败');
      }

      const aria2res = await addUri(fileData.url.url, fileData.file_name);
      if (aria2res.result) {
        settingStore.downloadSetting.downloadList.unshift({
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
        startPolling();
      }
    } catch (e) {
      console.error('重试下载失败:', e);
      throw e;
    }
  };

  /**
   * 删除下载任务
   */
  const removeTask = async (downloadFile: DownLoadFile) => {
    try {
      if (downloadFile.status === 'active' || downloadFile.status === 'waiting') {
        await forceRemove(downloadFile.gid);
      }
      if (!downloadFile.gid.startsWith('failed-')) {
        await removeDownloadResult(downloadFile.gid).catch(() => {});
      }
    } catch (e) {
      console.error('移除aria2任务失败:', e);
    }

    const index = settingStore.downloadSetting.downloadList.findIndex(
      (d) => d.gid === downloadFile.gid,
    );
    if (index !== -1) {
      settingStore.downloadSetting.downloadList.splice(index, 1);
    }
  };

  /**
   * 获取下载队列状态
   */
  const queueStatus = computed(() => ({
    queueLength: downloadQueue.value.length,
    isProcessing: isProcessing.value,
    isPolling: isPolling.value,
  }));

  /**
   * 获取下载统计
   */
  const downloadStats = computed(() => {
    const list = settingStore.downloadSetting.downloadList;
    const active = list.filter((d) => d.status === 'active');
    const totalSpeed = active.reduce((sum, d) => sum + (d.downloadSpeed || 0), 0);
    const completed = list.filter((d) => d.status === 'complete').length;
    const failed = list.filter((d) => d.status === 'error').length;
    const paused = list.filter((d) => d.status === 'paused').length;
    const waiting = list.filter((d) => d.status === 'waiting').length;

    return {
      activeCount: active.length,
      totalSpeed,
      completed,
      failed,
      paused,
      waiting,
      total: list.length,
    };
  });

  // 组件挂载时，如果有活跃任务则自动启动轮询（tryOnMounted 更安全，非 setup 上下文不会报错）
  tryOnMounted(() => {
    const hasActive = settingStore.downloadSetting.downloadList.some(
      (d) => d.status === 'active' || d.status === 'waiting' || d.status === 'paused' || !d.status,
    );
    if (hasActive) {
      startPolling();
    }
  });

  // useTimeoutPoll 会在 effect scope 销毁时自动清理，无需手动 onUnmounted

  return {
    download,
    batchDownload,
    retryDownload,
    removeTask,
    startPolling,
    stopPolling,
    syncDownloadStatus,
    queueStatus,
    downloadStats,
  };
});
