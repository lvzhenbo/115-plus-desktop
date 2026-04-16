import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { addFolder } from '@/api/file';
import { uploadGetToken, uploadInit, uploadResume } from '@/api/upload';
import type { UploadInitParams } from '@/api/types/upload';
import { useSettingStore } from '@/store/setting';
import { getBackoffDelay, isRateLimitError, sleep } from '@/utils/rateLimit';

// 上传列表的状态机完全以后端为准，前端只消费这些状态并做交互分发。
export type UploadStatus =
  | 'pending'
  | 'hashing'
  | 'uploading'
  | 'pausing'
  | 'paused'
  | 'complete'
  | 'error'
  | 'cancelled';

// Rust 存储层同步给前端的上传任务快照。
export interface UploadFile {
  id: string;
  fileName: string;
  filePath: string;
  fileSize: number;
  targetCid: string;
  targetPath?: string;
  sha1?: string;
  preSha1?: string;
  pickCode?: string;
  status: UploadStatus;
  progress: number;
  /** 由前端 upload:progress 事件填充，不再来自后端 DB */
  uploadSpeed?: number;
  etaSecs?: number;
  errorMessage?: string;
  createdAt?: number;
  completedAt?: number;
  isFolder?: boolean;
  parentId?: string;
  totalFiles?: number;
  completedFiles?: number;
  failedFiles?: number;
  ossBucket?: string;
  ossObject?: string;
  ossEndpoint?: string;
  callback?: string;
  callbackVar?: string;
  uploadedSize?: number;
  fileId?: string;
  ossUploadId?: string;
}

// 上传列表顶部摘要信息。
interface UploadQueueStatus {
  queueLength: number;
  isProcessing: boolean;
}

// 上传列表统计信息。
interface UploadStats {
  activeCount: number;
  totalSpeed: number;
  completed: number;
  failed: number;
  paused: number;
  total: number;
}

type UploadTaskAction = 'pause' | 'resume' | 'retry' | 'remove';
type UploadBatchAction = 'idle' | 'pausing-all' | 'resuming-all';

// 这些集合只服务于 UI 统计，不参与真实状态切换。
const ACTIVE_UPLOAD_STATUS_SET = new Set<UploadStatus>([
  'pending',
  'hashing',
  'uploading',
  'pausing',
]);
const PROCESSING_UPLOAD_STATUS_SET = new Set<UploadStatus>(['hashing', 'uploading', 'pausing']);
const PRESERVED_UPLOAD_STATUS_SET = new Set<UploadStatus>(['uploading', 'pausing', 'paused']);

/** upload:progress 事件的单项进度快照 (camelCase, 来自 Rust UploadProgressItem) */
interface UploadProgressItem {
  taskId: string;
  uploadedSize: number;
  totalSize: number;
  speed: number;
  etaSecs?: number;
  isFolder?: boolean;
}

/** progressCache 中的快照，防止 state-sync 进度回跳 */
interface UploadProgressSnapshot {
  speed: number;
  progress: number;
  etaSecs?: number;
}

// 文件和文件夹在后端对应的是不同 command，这里做一次前端映射收口。
const TASK_ACTION_COMMANDS: Record<UploadTaskAction, { file: string; folder: string }> = {
  pause: {
    file: 'upload_pause_task',
    folder: 'upload_pause_folder',
  },
  resume: {
    file: 'upload_resume_task',
    folder: 'upload_resume_folder',
  },
  retry: {
    file: 'upload_retry_task',
    folder: 'upload_retry_folder',
  },
  remove: {
    file: 'upload_remove_task',
    folder: 'upload_remove_folder',
  },
};

// 所有上传相关 invoke 都统一经过这里，便于后续替换和类型收口。
const invokeUploadCommand = async <T = void>(
  command: string,
  payload?: Record<string, unknown>,
): Promise<T> => {
  return payload ? invoke<T>(command, payload) : invoke<T>(command);
};

// 主列表只展示顶层任务；文件夹内部子任务由后端聚合后体现在父任务上。
const getTopLevelUploads = async (): Promise<UploadFile[]> => {
  return invokeUploadCommand<UploadFile[]>('upload_get_top_level_tasks');
};

// `upload:api-needed` 事件的前端解析结构；真实字段由 Rust 按 camel/snake 两种形式兼容发出。
interface LocalUploadFileInput {
  path: string;
  name: string;
  size: number;
}

interface UploadApiNeededEvent {
  kind: 'init' | 'resume' | 'token' | 'createFolder';
  requestId?: string;
  request_id?: string;
  [key: string]: unknown;
}

// 兼容 Rust 事件载荷中的 camelCase / snake_case 字段。
const getField = (payload: Record<string, unknown>, camelKey: string, snakeKey = camelKey) =>
  payload[camelKey] ?? payload[snakeKey];

// 下面这组 helper 用于在前端事件入口处尽早校验 payload，避免把脏数据带进具体 API 调用。
const requireStringField = (
  payload: Record<string, unknown>,
  camelKey: string,
  snakeKey = camelKey,
) => {
  const value = getField(payload, camelKey, snakeKey);
  if (typeof value !== 'string') {
    throw new Error(`缺少字段 ${camelKey}`);
  }
  return value;
};

const requireNumberField = (
  payload: Record<string, unknown>,
  camelKey: string,
  snakeKey = camelKey,
) => {
  const value = getField(payload, camelKey, snakeKey);
  if (typeof value !== 'number') {
    throw new Error(`缺少字段 ${camelKey}`);
  }
  return value;
};

const optionalStringField = (
  payload: Record<string, unknown>,
  camelKey: string,
  snakeKey = camelKey,
) => {
  const value = getField(payload, camelKey, snakeKey);
  return typeof value === 'string' ? value : undefined;
};

const assignIfDefined = <T extends object, K extends keyof T>(
  target: T,
  key: K,
  value: T[K] | undefined,
) => {
  if (value !== undefined) {
    target[key] = value;
  }
};

const getErrorMessage = (error: unknown) => {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;

  if (error && typeof error === 'object') {
    const data = error as {
      message?: unknown;
      error?: unknown;
      code?: unknown;
      errno?: unknown;
    };

    const message =
      (typeof data.message === 'string' && data.message) ||
      (typeof data.error === 'string' && data.error) ||
      null;
    const code =
      typeof data.code === 'number'
        ? data.code
        : typeof data.errno === 'number'
          ? data.errno
          : null;

    if (message && code !== null) {
      return `${message} (code=${code})`;
    }

    if (message) {
      return message;
    }

    try {
      return JSON.stringify(error);
    } catch {
      return Object.prototype.toString.call(error);
    }
  }

  return String(error);
};

const logUploadManagerError = (message: string, error: unknown) => {
  console.error(message, error);
};

/**
 * 上传管理器。
 *
 * 当前版本里它是一个“薄桥接层”：
 * - 列表状态来自 Rust `upload:*` 事件
 * - 用户操作转发为 `upload_*` command
 * - 115 上传接口仍由前端调用，再把结果回填给 Rust 调度器
 */
export const useUploadManager = createSharedComposable(() => {
  const settingStore = useSettingStore();

  // `displayList` 是当前 UI 的唯一来源；progressCache 只用于填补 state-sync 间隙。
  const displayList = shallowRef<UploadFile[]>([]);
  const progressCache = new Map<string, UploadProgressSnapshot>();
  const unlisteners: UnlistenFn[] = [];
  const stopHandles: Array<() => void> = [];
  const batchAction = ref<UploadBatchAction>('idle');

  let listenerPromise: Promise<void> | null = null;
  let initPromise: Promise<void> | null = null;
  let batchActionPromise: Promise<void> | null = null;

  const ensureNoBatchActionInFlight = () => {
    if (batchAction.value !== 'idle') {
      throw new Error('批量暂停/继续进行中，请稍候');
    }
  };

  const runBatchAction = async (
    action: Exclude<UploadBatchAction, 'idle'>,
    operation: () => Promise<void>,
  ) => {
    if (batchActionPromise) {
      throw new Error('批量暂停/继续进行中，请稍候');
    }

    batchAction.value = action;
    batchActionPromise = operation().finally(() => {
      batchAction.value = 'idle';
      batchActionPromise = null;
    });

    await batchActionPromise;
  };

  // 主动刷新只在初始化/清理后使用，正常更新路径依赖 `upload:state-sync` 推送。
  const refreshDisplayList = async () => {
    displayList.value = await getTopLevelUploads();
  };

  // Rust 等待前端回填 115 API 结果时，统一走这两个桥接 command。
  const provideApiResponse = async (requestId: string, payload: unknown) => {
    await invokeUploadCommand('upload_provide_api_response', { requestId, payload });
  };

  const provideApiError = async (requestId: string, errorMessage: string) => {
    await invokeUploadCommand('upload_provide_api_error', { requestId, errorMessage });
  };

  const syncMaxConcurrent = async (n = settingStore.uploadSetting.maxConcurrent) => {
    await invokeUploadCommand('upload_set_max_concurrent', { n });
  };

  const syncMaxRetry = async (n = settingStore.uploadSetting.maxRetry) => {
    await invokeUploadCommand('upload_set_max_retry', { n });
  };

  const syncUploadSettings = async () => {
    await Promise.all([syncMaxConcurrent(), syncMaxRetry()]);
  };

  // 远端目录创建仍复用现有前端 API，并带上限流退避，避免文件夹批量展开时击穿接口。
  const createRemoteFolder = async (fileName: string, parentCid: string) => {
    const maxRetry = settingStore.uploadSetting.maxRetry;
    for (let attempt = 0; attempt <= maxRetry; attempt++) {
      try {
        const response = await addFolder({ file_name: fileName, pid: parentCid });
        if (!response.data) {
          throw new Error('创建文件夹失败：无返回数据');
        }
        return response.data;
      } catch (error) {
        if (isRateLimitError(error) && attempt < maxRetry) {
          await sleep(getBackoffDelay(attempt));
          continue;
        }
        throw error;
      }
    }

    throw new Error('创建文件夹失败：超过最大重试次数');
  };

  // Rust 只描述“需要什么接口结果”，具体 HTTP 请求仍由前端完成。
  const respondToApiRequest = async (payload: UploadApiNeededEvent) => {
    const requestId = requireStringField(payload, 'requestId', 'request_id');

    try {
      switch (payload.kind) {
        case 'init': {
          const request: UploadInitParams = {
            file_name: requireStringField(payload, 'fileName', 'file_name'),
            file_size: requireNumberField(payload, 'fileSize', 'file_size'),
            target: requireStringField(payload, 'target'),
            fileid: requireStringField(payload, 'fileid'),
          };
          assignIfDefined(request, 'preid', optionalStringField(payload, 'preid'));
          assignIfDefined(
            request,
            'pick_code',
            optionalStringField(payload, 'pickCode', 'pick_code'),
          );
          assignIfDefined(request, 'sign_key', optionalStringField(payload, 'signKey', 'sign_key'));
          assignIfDefined(request, 'sign_val', optionalStringField(payload, 'signVal', 'sign_val'));

          const response = await uploadInit(request);
          await provideApiResponse(requestId, response.data);
          return;
        }
        case 'resume': {
          const response = await uploadResume({
            file_size: requireNumberField(payload, 'fileSize', 'file_size'),
            target: requireStringField(payload, 'target'),
            fileid: requireStringField(payload, 'fileid'),
            pick_code: requireStringField(payload, 'pickCode', 'pick_code'),
          });
          await provideApiResponse(requestId, response.data);
          return;
        }
        case 'token': {
          const response = await uploadGetToken();
          await provideApiResponse(requestId, response.data);
          return;
        }
        case 'createFolder': {
          const folder = await createRemoteFolder(
            requireStringField(payload, 'fileName', 'file_name'),
            requireStringField(payload, 'parentCid', 'parent_cid'),
          );
          await provideApiResponse(requestId, folder);
          return;
        }
        default:
          throw new Error(`未知的上传接口请求类型: ${payload.kind}`);
      }
    } catch (error) {
      console.error('处理上传接口请求失败:', payload.kind, error);
      await provideApiError(requestId, getErrorMessage(error));
    }
  };

  const updateTask = (id: string, updater: (task: UploadFile) => void) => {
    const target = displayList.value.find((item) => item.id === id);
    if (!target) return;
    updater(target);
  };

  // 当 state-sync 回来的任务仍处于活跃态时，用最近一次进度快照补齐 UI，避免"速度归零"和"进度倒退"。
  const applyProgressSnapshot = (task: UploadFile, snapshot: UploadProgressSnapshot) => {
    task.progress = snapshot.progress;

    if (task.status === 'paused') {
      task.uploadSpeed = 0;
      task.etaSecs = undefined;
    } else {
      task.uploadSpeed = snapshot.speed;
      task.etaSecs = snapshot.etaSecs;
    }
  };

  // 清理已经不在列表里的缓存快照，避免长时间运行后 map 无界增长。
  const pruneProgressCache = (tasks: UploadFile[]) => {
    const currentIds = new Set(tasks.map((item) => item.id));
    for (const id of progressCache.keys()) {
      if (!currentIds.has(id)) {
        progressCache.delete(id);
      }
    }
  };

  // `upload:state-sync` 是顶层任务列表的权威同步事件。
  const handleStateSync = (tasks: UploadFile[]) => {
    displayList.value = tasks;

    for (const item of displayList.value) {
      const snapshot = progressCache.get(item.id);
      if (snapshot && PRESERVED_UPLOAD_STATUS_SET.has(item.status)) {
        applyProgressSnapshot(item, snapshot);
      }
    }

    pruneProgressCache(displayList.value);
  };

  // `upload:progress` 只提供瞬时指标（速度/ETA/进度），真正的结构化列表仍以 state-sync 为准。
  const handleProgress = (items: UploadProgressItem[]) => {
    for (const item of items) {
      const progress =
        item.totalSize > 0
          ? Math.min(100, Math.round((item.uploadedSize / item.totalSize) * 10000) / 100)
          : 0;
      const eta = item.etaSecs != null ? Math.ceil(item.etaSecs) : undefined;

      progressCache.set(item.taskId, {
        speed: item.speed,
        progress,
        etaSecs: eta,
      });

      updateTask(item.taskId, (task) => {
        task.uploadSpeed = item.speed;
        task.progress = progress;
        task.etaSecs = eta;
      });
    }
  };

  // 监听器只允许初始化一次，防止重复进入页面时叠加订阅，导致同一事件被处理多次。
  const setupUploadListeners = async () => {
    if (!listenerPromise) {
      listenerPromise = Promise.all([
        listen<UploadFile[]>('upload:state-sync', (event) => {
          handleStateSync(event.payload);
        }),
        listen<UploadProgressItem[]>('upload:progress', (event) => {
          handleProgress(event.payload);
        }),
        listen<UploadApiNeededEvent>('upload:api-needed', (event) => {
          void respondToApiRequest(event.payload);
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

  // 设置同步和事件监听一样，只初始化一套，避免 shared composable 重复挂 watcher。
  const setupSettingSync = () => {
    if (stopHandles.length > 0) {
      return;
    }

    stopHandles.push(
      watch(
        () => settingStore.uploadSetting.maxConcurrent,
        (n) => {
          void syncMaxConcurrent(n).catch((error) => {
            logUploadManagerError('同步上传并发设置失败:', error);
          });
        },
      ),
      watch(
        () => settingStore.uploadSetting.maxRetry,
        (n) => {
          void syncMaxRetry(n).catch((error) => {
            logUploadManagerError('同步上传重试设置失败:', error);
          });
        },
      ),
    );
  };

  // 文件/文件夹动作在这里统一分派，视图层不必知道各自对应哪个 Rust command。
  const runTaskAction = async (action: UploadTaskAction, task: UploadFile) => {
    const command = TASK_ACTION_COMMANDS[action];
    if (task.isFolder) {
      await invokeUploadCommand(command.folder, { parentId: task.id });
      return;
    }

    await invokeUploadCommand(command.file, { id: task.id });
  };

  // 当最后一个使用者卸载时，清掉事件监听和设置 watcher，确保 shared composable 可安全重建。
  const dispose = () => {
    listenerPromise = null;
    initPromise = null;

    for (const unlisten of unlisteners.splice(0)) {
      unlisten();
    }

    for (const stop of stopHandles.splice(0)) {
      stop();
    }
  };

  tryOnScopeDispose(dispose);

  // 单文件上传只是多文件上传的一个便捷包装。
  const uploadFile = async (
    filePath: string,
    fileName: string,
    fileSize: number,
    targetCid: string,
  ) => {
    await uploadFiles([{ path: filePath, name: fileName, size: fileSize }], targetCid);
  };

  const uploadFiles = async (files: LocalUploadFileInput[], targetCid: string) => {
    ensureNoBatchActionInFlight();
    await invokeUploadCommand('upload_enqueue_files', { files, targetCid });
  };

  // 文件夹上传真正的目录扫描和远端建目录都在 Rust 调度器里完成。
  const uploadFolder = async (folderPath: string, folderName: string, targetCid: string) => {
    ensureNoBatchActionInFlight();
    await invokeUploadCommand('upload_enqueue_folder', { folderPath, folderName, targetCid });
  };

  const pauseTask = async (task: UploadFile) => {
    ensureNoBatchActionInFlight();
    await runTaskAction('pause', task);
  };

  const resumeTask = async (task: UploadFile) => {
    ensureNoBatchActionInFlight();
    await runTaskAction('resume', task);
  };

  const retryTask = async (task: UploadFile) => {
    ensureNoBatchActionInFlight();
    await runTaskAction('retry', task);
  };

  const removeTask = async (task: UploadFile) => {
    ensureNoBatchActionInFlight();
    await runTaskAction('remove', task);
    displayList.value = displayList.value.filter((item) => item.id !== task.id);
  };

  const clearFinished = async () => {
    ensureNoBatchActionInFlight();
    await invokeUploadCommand('upload_delete_finished_tasks');
    await refreshDisplayList();
  };

  const pauseAllTasks = async () => {
    await runBatchAction('pausing-all', () => invokeUploadCommand('upload_pause_all'));
  };

  const resumeAllTasks = async () => {
    await runBatchAction('resuming-all', () => invokeUploadCommand('upload_resume_all'));
  };

  const isBatchOperating = computed(() => batchAction.value !== 'idle');
  const isPausingAll = computed(() => batchAction.value === 'pausing-all');
  const isResumingAll = computed(() => batchAction.value === 'resuming-all');

  // 这些 computed 只做展示聚合，不持有真实任务状态。
  const queueStatus = computed<UploadQueueStatus>(() => {
    const list = displayList.value;
    const queueLength = list.filter((item) => item.status === 'pending').length;
    const isProcessing = list.some((item) => PROCESSING_UPLOAD_STATUS_SET.has(item.status));
    return { queueLength, isProcessing };
  });

  const uploadStats = computed<UploadStats>(() => {
    const list = displayList.value;
    const activeCount = list.filter((item) => ACTIVE_UPLOAD_STATUS_SET.has(item.status)).length;
    const totalSpeed = list
      .filter((item) => item.status === 'uploading')
      .reduce((sum, item) => sum + (item.uploadSpeed || 0), 0);
    return {
      activeCount,
      totalSpeed,
      completed: list.filter((item) => item.status === 'complete').length,
      failed: list.filter((item) => item.status === 'error').length,
      paused: list.filter((item) => item.status === 'paused').length,
      total: list.length,
    };
  });

  // 与下载管理器保持一致，布局层和更新器都直接消费这个布尔值。
  const hasActiveUploads = computed(() =>
    displayList.value.some((item) => ACTIVE_UPLOAD_STATUS_SET.has(item.status)),
  );

  // `init` 设计成幂等入口，页面重复调用只会复用同一个初始化 promise。
  const init = async () => {
    if (!initPromise) {
      initPromise = (async () => {
        await setupUploadListeners();
        await refreshDisplayList();
        await syncUploadSettings();
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
    uploadFile,
    uploadFiles,
    uploadFolder,
    pauseTask,
    resumeTask,
    retryTask,
    removeTask,
    clearFinished,
    pauseAllTasks,
    resumeAllTasks,
    isBatchOperating,
    isPausingAll,
    isResumingAll,
    queueStatus,
    uploadStats,
    hasActiveUploads,
  };
});
