import { uploadInit, uploadGetToken, uploadResume } from '@/api/upload';
import { addFolder } from '@/api/file';
import {
  insertUpload,
  updateUpload,
  deleteUpload,
  deleteChildUploads,
  deleteFinishedUploads,
  getAllUploads,
  getChildUploads,
  getTopLevelUploads,
  getActiveUploads,
  getUploadById,
  type UploadFile,
} from '@/db/uploads';
import { listen } from '@tauri-apps/api/event';

/** 上传队列项 */
interface UploadQueueItem {
  filePath: string;
  fileName: string;
  fileSize: number;
  targetCid: string;
  retryCount: number;
  parentId?: string;
  /** 已关联的数据库记录 ID，重试时复用 */
  dbId?: string;
  /** 已计算的文件 SHA1（断点续传时复用，跳过重新计算） */
  sha1?: string;
  /** 已计算的文件前 128K SHA1（断点续传时复用） */
  preSha1?: string;
  /** 上一次上传返回的 pick_code（断点续传时使用） */
  pickCode?: string;
  /** OSS 分片上传 ID（断点续传时复用） */
  ossUploadId?: string;
}

/** 最大重试次数 */
const MAX_RETRY = 3;
/** 队列处理延迟(ms) */
const QUEUE_DELAY = 1000;

/** 状态轮询间隔(ms) */
const POLL_INTERVAL = 2000;
/** 限流退避基础延迟(ms) */
const BACKOFF_BASE = 3000;
/** 限流退避最大延迟(ms) */
const BACKOFF_MAX = 60000;

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

const generateId = () => `upload-${Date.now()}-${Math.random().toString(36).slice(2)}`;

/**
 * 上传管理器
 *
 * - SQLite 持久化上传列表，响应式 `displayList` 驱动 UI
 * - 使用 Rust 后端进行 SHA1 计算和 OSS 上传
 * - 支持暂停、继续、重试、删除操作
 * - 支持文件和文件夹上传
 */
export const useUploadManager = createSharedComposable(() => {
  const displayList = ref<UploadFile[]>([]);
  const uploadQueue = ref<UploadQueueItem[]>([]);
  const isProcessing = ref(false);

  /** 从数据库刷新顶层列表 */
  const refreshDisplayList = async () => {
    displayList.value = await getTopLevelUploads();
  };

  // ---------- 进度事件监听 ----------

  let progressListenerSetup = false;

  const setupProgressListener = async () => {
    if (progressListenerSetup) return;
    progressListenerSetup = true;

    await listen<{
      upload_id: string;
      uploaded_size: number;
      total_size: number;
      part_number: number;
      total_parts: number;
      status: string;
    }>('upload-progress', async (event) => {
      const { upload_id, uploaded_size, total_size, status } = event.payload;

      // 检查当前任务状态，如果已暂停或已取消则忽略滞后的进度事件
      const currentRecord = await getUploadById(upload_id);
      if (
        currentRecord &&
        (currentRecord.status === 'paused' || currentRecord.status === 'cancelled')
      ) {
        return;
      }

      if (status === 'complete') {
        await updateUpload(upload_id, {
          status: 'complete',
          progress: 100,
          uploadSpeed: 0,
          uploadedSize: total_size,
          completedAt: Date.now(),
          ossUploadId: undefined, // 上传完成后清除 ossUploadId
        });
      } else {
        const progress =
          total_size > 0 ? Math.round((uploaded_size / total_size) * 10000) / 100 : 0;
        await updateUpload(upload_id, {
          progress,
          uploadedSize: uploaded_size,
          status: 'uploading',
        });
      }

      await aggregateFolderStatuses();
      await refreshDisplayList();
    });

    // 监听 OSS 分片上传初始化事件，保存 oss_upload_id 用于断点续传
    await listen<{
      upload_id: string;
      oss_upload_id: string;
    }>('upload-oss-init', async (event) => {
      const { upload_id, oss_upload_id } = event.payload;
      await updateUpload(upload_id, { ossUploadId: oss_upload_id });
    });
  };

  // ---------- 状态轮询 ----------

  const {
    pause: stopPolling,
    resume: startPolling,
    isActive: isPolling,
  } = useTimeoutPoll(pollUploadStatus, POLL_INTERVAL, { immediate: false });

  /** 轮询上传状态，聚合文件夹进度 */
  async function pollUploadStatus() {
    await aggregateFolderStatuses();
    await refreshDisplayList();

    // 无活跃任务时停止轮询
    const active = await getActiveUploads();
    if (active.length === 0 && uploadQueue.value.length === 0 && !isProcessing.value) {
      stopPolling();
    }
  }

  /** 聚合文件夹内子任务的进度、完成状态 */
  async function aggregateFolderStatuses() {
    const allItems = await getAllUploads();
    const folders = allItems.filter((d) => d.isFolder && d.status !== 'cancelled');

    for (const folder of folders) {
      const children = allItems.filter((d) => d.parentId === folder.id);
      if (children.length === 0 && folder.status !== 'complete' && folder.status !== 'error')
        continue;

      const completed = children.filter((d) => d.status === 'complete').length;
      const failed = children.filter((d) => d.status === 'error').length;
      const paused = children.filter((d) => d.status === 'paused').length;
      const activeChildren = children.filter(
        (d) => d.status === 'uploading' || d.status === 'hashing' || d.status === 'pending',
      );

      const totalSize = children.reduce((sum, d) => sum + (d.fileSize || 0), 0);
      const completedSize = children.reduce((sum, d) => {
        if (d.status === 'complete') return sum + (d.fileSize || 0);
        if (d.progress && d.fileSize) return sum + (d.fileSize * d.progress) / 100;
        return sum;
      }, 0);

      const uploadSpeed = activeChildren.reduce((sum, d) => sum + (d.uploadSpeed || 0), 0);

      const updates: Partial<UploadFile> = {
        completedFiles: completed,
        failedFiles: failed,
        fileSize: totalSize > 0 ? totalSize : folder.fileSize,
        progress: totalSize > 0 ? Math.round((completedSize / totalSize) * 10000) / 100 : 0,
        uploadSpeed,
      };

      if (completed + failed === children.length && children.length > 0) {
        if (failed > 0) {
          updates.status = 'error';
          updates.errorMessage = `${failed} 个文件上传失败`;
          updates.uploadSpeed = 0;
        } else {
          updates.status = 'complete';
          updates.completedAt = folder.completedAt ?? Date.now();
          updates.uploadSpeed = 0;
        }
      } else if (paused > 0 && activeChildren.length === 0) {
        updates.status = 'paused';
      } else if (activeChildren.length > 0) {
        updates.status = 'uploading';
      }

      await updateUpload(folder.id, updates);
    }
  }

  // ---------- 队列与上传 ----------

  /** 将单个文件加入上传队列 */
  const enqueueFile = (
    filePath: string,
    fileName: string,
    fileSize: number,
    targetCid: string,
    parentId?: string,
  ) => {
    uploadQueue.value.push({
      filePath,
      fileName,
      fileSize,
      targetCid,
      retryCount: 0,
      parentId,
    });
    processQueue();
  };

  /** 上传单个本地文件 */
  const uploadFile = async (
    filePath: string,
    fileName: string,
    fileSize: number,
    targetCid: string,
  ) => {
    enqueueFile(filePath, fileName, fileSize, targetCid);
    startPolling();
  };

  /** 在115云端创建文件夹（带限流退避重试） */
  const createRemoteFolder = async (name: string, parentCid: string): Promise<string> => {
    for (let attempt = 0; attempt <= MAX_RETRY; attempt++) {
      try {
        const res = await addFolder({ file_name: name, pid: parentCid });
        if (res.data) {
          return res.data.file_id;
        }
        throw new Error(`创建文件夹 "${name}" 失败：无返回数据`);
      } catch (err) {
        if (isRateLimitError(err) && attempt < MAX_RETRY) {
          const delay = getBackoffDelay(attempt);
          console.warn(`创建文件夹 "${name}" 被限流，${delay / 1000}s 后重试第 ${attempt + 1} 次`);
          await sleep(delay);
        } else {
          throw err;
        }
      }
    }
    throw new Error(`创建文件夹 "${name}" 超过最大重试次数`);
  };

  /** 上传本地文件夹 */
  const uploadFolder = async (folderPath: string, folderName: string, targetCid: string) => {
    const folderId = generateId();

    const folderEntry: UploadFile = {
      id: folderId,
      fileName: folderName,
      filePath: folderPath,
      fileSize: 0,
      targetCid,
      status: 'pending',
      progress: 0,
      uploadSpeed: 0,
      isFolder: true,
      totalFiles: 0,
      completedFiles: 0,
      failedFiles: 0,
      createdAt: Date.now(),
    };
    await insertUpload(folderEntry);
    await refreshDisplayList();

    // 递归收集文件夹下所有文件
    const allFiles: { path: string; name: string; size: number; relativePath: string }[] = [];
    try {
      await collectLocalFolderFiles(folderPath, allFiles);
    } catch (e) {
      console.error('收集文件夹文件失败:', e);
      await updateUpload(folderId, {
        status: 'error',
        errorMessage: '收集文件列表失败',
      });
      await refreshDisplayList();
      return;
    }

    const totalSize = allFiles.reduce((sum, f) => sum + f.size, 0);
    await updateUpload(folderId, {
      totalFiles: allFiles.length,
      fileSize: totalSize,
      status: 'uploading',
    });

    if (allFiles.length === 0) {
      await updateUpload(folderId, { status: 'complete', completedAt: Date.now() });
      await refreshDisplayList();
      return;
    }

    // 在115云端创建根文件夹
    let rootFolderCid: string;
    try {
      rootFolderCid = await createRemoteFolder(folderName, targetCid);
    } catch (e) {
      console.error('创建远程根文件夹失败:', e);
      await updateUpload(folderId, {
        status: 'error',
        errorMessage: `创建根文件夹失败: ${e}`,
      });
      await refreshDisplayList();
      return;
    }

    // 从文件的 relativePath 中提取所有需要创建的子目录路径
    const dirPaths = new Set<string>();
    for (const file of allFiles) {
      const parts = file.relativePath.replace(/\\/g, '/').split('/');
      // 只取目录部分（去掉最后的文件名）
      for (let i = 1; i < parts.length; i++) {
        dirPaths.add(parts.slice(0, i).join('/'));
      }
    }

    // 按层级排序，确保父目录先创建
    const sortedDirs = Array.from(dirPaths).sort(
      (a, b) => a.split('/').length - b.split('/').length || a.localeCompare(b),
    );

    // 在115上创建对应的文件夹结构
    // key: 相对路径, value: 115上的 cid
    const dirCidMap = new Map<string, string>();
    try {
      for (let idx = 0; idx < sortedDirs.length; idx++) {
        const dirPath = sortedDirs[idx]!;
        const parts = dirPath.split('/');
        const dirName = parts[parts.length - 1]!;
        const parentRelPath = parts.slice(0, -1).join('/');
        let parentCid: string;
        if (parentRelPath) {
          const mapped = dirCidMap.get(parentRelPath);
          if (!mapped) {
            throw new Error(`找不到父目录 "${parentRelPath}" 的 CID`);
          }
          parentCid = mapped;
        } else {
          parentCid = rootFolderCid;
        }

        // 主动限流：每次调用前等待，避免触发接口限流
        if (idx > 0) {
          await sleep(QUEUE_DELAY);
        }

        const cid = await createRemoteFolder(dirName, parentCid);
        dirCidMap.set(dirPath, cid);
      }
    } catch (e) {
      console.error('创建远程文件夹结构失败:', e);
      await updateUpload(folderId, {
        status: 'error',
        errorMessage: `创建文件夹结构失败: ${e}`,
      });
      await refreshDisplayList();
      return;
    }

    // 将文件加入队列，根据 relativePath 确定正确的 targetCid
    // 预先插入 DB 记录（状态为 pending），确保文件夹聚合状态正确
    for (const file of allFiles) {
      const relParts = file.relativePath.replace(/\\/g, '/').split('/');
      const fileDirPath = relParts.slice(0, -1).join('/');
      const fileTargetCid = fileDirPath ? dirCidMap.get(fileDirPath)! : rootFolderCid!;

      const fileId = generateId();
      await insertUpload({
        id: fileId,
        fileName: file.name,
        filePath: file.path,
        fileSize: file.size,
        targetCid: fileTargetCid,
        status: 'pending',
        progress: 0,
        uploadSpeed: 0,
        parentId: folderId,
        createdAt: Date.now(),
      });

      uploadQueue.value.push({
        filePath: file.path,
        fileName: file.name,
        fileSize: file.size,
        targetCid: fileTargetCid,
        retryCount: 0,
        parentId: folderId,
        dbId: fileId,
      });
    }

    await refreshDisplayList();
    processQueue();
    startPolling();
  };

  /** 递归收集本地文件夹下所有文件 */
  const collectLocalFolderFiles = async (
    dirPath: string,
    result: { path: string; name: string; size: number; relativePath: string }[],
  ) => {
    const files: { path: string; name: string; size: number; is_dir: boolean }[] = await invoke(
      'scan_directory',
      { dirPath },
    );
    for (const file of files) {
      if (!file.is_dir) {
        result.push({
          path: file.path,
          name: file.name,
          size: file.size,
          relativePath: file.path.replace(dirPath, '').replace(/^[\\/]/, ''),
        });
      }
    }
  };

  /** 逐项消费上传队列 */
  const processQueue = async () => {
    if (isProcessing.value) return;
    isProcessing.value = true;

    try {
      while (uploadQueue.value.length > 0) {
        const item = uploadQueue.value.shift()!;

        // 跳过已暂停或已取消的任务（防止暂停后队列残留的竞态情况）
        if (item.dbId) {
          const record = await getUploadById(item.dbId);
          if (record && (record.status === 'paused' || record.status === 'cancelled')) {
            continue;
          }
        }

        try {
          await uploadSingleFile(item);
        } catch (error) {
          // uploadSingleFile 内部已在 DB 中标记 error，此处只处理重试逻辑
          if (item.retryCount < MAX_RETRY) {
            item.retryCount++;
            const isRateLimit = isRateLimitError(error);
            const delay = isRateLimit
              ? getBackoffDelay(item.retryCount)
              : getBackoffDelay(Math.max(0, item.retryCount - 1));

            console.warn(
              `上传失败${isRateLimit ? '(限流)' : ''}，${delay / 1000}s 后重试第 ${item.retryCount} 次: ${item.fileName}`,
              error,
            );

            if (isRateLimit) {
              await sleep(delay);
            }
            uploadQueue.value.unshift(item);
          } else {
            // 已超过最大重试次数，uploadSingleFile 中已创建了 error 记录，无需再创建
            console.error(`上传失败，已超过最大重试次数: ${item.fileName}`, error);
          }
          await refreshDisplayList();
        }

        if (uploadQueue.value.length > 0) {
          await sleep(QUEUE_DELAY);
        }
      }
    } finally {
      isProcessing.value = false;
    }
  };

  /** 上传单个文件的完整流程 */
  const uploadSingleFile = async (item: UploadQueueItem) => {
    const { filePath, fileName, fileSize, targetCid, parentId } = item;
    const isRetry = !!item.dbId;
    const id = item.dbId || generateId();
    item.dbId = id;

    // 1. 插入或复用上传记录
    if (!isRetry) {
      await insertUpload({
        id,
        fileName,
        filePath,
        fileSize,
        targetCid,
        status: 'hashing',
        progress: 0,
        uploadSpeed: 0,
        parentId,
        createdAt: Date.now(),
      });
    } else {
      // 重试时复用同一条记录，重置状态
      await updateUpload(id, {
        status: 'hashing',
        progress: 0,
        uploadSpeed: 0,
        errorMessage: undefined,
        uploadedSize: 0,
      });
    }
    await refreshDisplayList();
    startPolling();

    // 2. 计算文件 SHA1（如果已有则复用，跳过重新计算）
    const target = `U_1_${targetCid}`;
    let sha1: string;
    let preSha1: string;

    if (item.sha1 && item.preSha1) {
      // 断点续传：复用已保存的哈希值
      sha1 = item.sha1;
      preSha1 = item.preSha1;
      await updateUpload(id, { sha1, preSha1 });
    } else {
      let hashResult: { sha1: string; pre_sha1: string };
      try {
        hashResult = await invoke('compute_file_hash', { filePath });
      } catch (e) {
        await updateUpload(id, {
          status: 'error',
          errorMessage: `计算文件哈希失败: ${e}`,
        });
        throw e;
      }
      sha1 = hashResult.sha1;
      preSha1 = hashResult.pre_sha1;
      await updateUpload(id, { sha1, preSha1 });
    }

    // 3. 断点续传：如果已有 pickCode，走 /open/upload/resume 接口
    if (item.pickCode) {
      try {
        await resumeUploadFlow(
          id,
          filePath,
          fileName,
          fileSize,
          target,
          sha1,
          item.pickCode,
          item.ossUploadId,
        );
        return;
      } catch (e) {
        // 续传接口失败，回退到完整 init 流程
        console.warn(`断点续传失败，回退到完整上传流程: ${fileName}`, e);
      }
    }

    // 4. 调用115上传初始化接口
    let initRes;
    try {
      initRes = await uploadInit({
        file_name: fileName,
        file_size: fileSize,
        target,
        fileid: sha1,
        preid: preSha1,
      });
    } catch (e) {
      await updateUpload(id, {
        status: 'error',
        errorMessage: `初始化上传失败: ${(e as Error)?.message || e}`,
      });
      throw e;
    }

    const initData = initRes.data;
    if (!initData) {
      await updateUpload(id, {
        status: 'error',
        errorMessage: '初始化上传失败：无返回数据',
      });
      throw new Error('初始化上传失败：无返回数据');
    }

    await updateUpload(id, { pickCode: initData.pick_code });

    // 处理二次认证
    if (initData.sign_key && initData.sign_check) {
      try {
        await handleSignCheck(id, fileName, filePath, fileSize, target, sha1, initData);
        return; // 二次认证后会重新调用上传
      } catch (e) {
        await updateUpload(id, {
          status: 'error',
          errorMessage: `二次认证失败: ${(e as Error)?.message || e}`,
        });
        throw e;
      }
    }

    // 5. 检查是否秒传
    if (initData.status === 2) {
      await updateUpload(id, {
        status: 'complete',
        progress: 100,
        completedAt: Date.now(),
        fileId: initData.file_id,
      });
      await refreshDisplayList();
      return;
    }

    // 6. 非秒传，获取上传凭证并上传到 OSS
    if (initData.status === 1 && initData.bucket && initData.object && initData.callback) {
      await doOssUpload(
        id,
        filePath,
        initData.bucket,
        initData.object,
        initData.callback.callback,
        initData.callback.callback_var,
      );
    }
  };

  /** 断点续传流程：调用 /open/upload/resume 获取 OSS 参数后上传 */
  const resumeUploadFlow = async (
    id: string,
    filePath: string,
    _fileName: string,
    fileSize: number,
    target: string,
    sha1: string,
    pickCode: string,
    ossUploadId?: string,
  ) => {
    // 读取 DB 中保存的 bucket/object，用于判断 resume 返回的是否相同
    const dbRecord = await getUploadById(id);
    const savedBucket = dbRecord?.ossBucket;
    const savedObject = dbRecord?.ossObject;

    const resumeRes = await uploadResume({
      file_size: fileSize,
      target,
      fileid: sha1,
      pick_code: pickCode,
    });

    const resumeData = resumeRes.data;
    if (!resumeData) {
      throw new Error('断点续传失败：无返回数据');
    }

    await updateUpload(id, { pickCode: resumeData.pick_code });

    if (resumeData.bucket && resumeData.object && resumeData.callback) {
      // 如果 resume 返回的 bucket/object 与保存的不同，说明 OSS 上传目标已变更
      // 此时旧的 ossUploadId 无效（它绑定了旧的 bucket/object），需要清除
      let validOssUploadId = ossUploadId;
      if (ossUploadId && (savedBucket !== resumeData.bucket || savedObject !== resumeData.object)) {
        console.warn(
          `断点续传目标已变更 (bucket: ${savedBucket} → ${resumeData.bucket}, object: ${savedObject} → ${resumeData.object})，清除旧的 ossUploadId`,
        );
        validOssUploadId = undefined;
        await updateUpload(id, { ossUploadId: undefined });
      }

      await doOssUpload(
        id,
        filePath,
        resumeData.bucket,
        resumeData.object,
        resumeData.callback.callback,
        resumeData.callback.callback_var,
        validOssUploadId,
      );
    } else {
      throw new Error('断点续传返回数据不完整');
    }
  };

  /** 获取上传凭证并上传文件到 OSS */
  const doOssUpload = async (
    id: string,
    filePath: string,
    bucket: string,
    object: string,
    callback: string,
    callbackVar: string,
    ossUploadId?: string,
  ) => {
    await updateUpload(id, {
      status: 'uploading',
      ossBucket: bucket,
      ossObject: object,
      callback,
      callbackVar,
    });

    // STS 凭证过期后自动刷新并重试（最多重试 3 次）
    const MAX_TOKEN_REFRESH = 3;
    let currentOssUploadId = ossUploadId;

    for (let tokenAttempt = 0; tokenAttempt <= MAX_TOKEN_REFRESH; tokenAttempt++) {
      // 获取 OSS 上传凭证
      let tokenRes;
      try {
        tokenRes = await uploadGetToken();
      } catch (e) {
        await updateUpload(id, {
          status: 'error',
          errorMessage: `获取上传凭证失败: ${(e as Error)?.message || e}`,
        });
        throw e;
      }

      const token = tokenRes.data;

      // 将 Expiration 转为毫秒时间戳传给 Rust，让 Rust 在每个分片上传前检查
      let tokenExpirationMs: number | null = null;
      if (token.Expiration) {
        const expDate = new Date(token.Expiration);
        if (!isNaN(expDate.getTime())) {
          tokenExpirationMs = expDate.getTime();
        }
      }

      // 从 DB 中获取最新的 ossUploadId（可能在上一轮中被 upload-oss-init 事件更新）
      if (tokenAttempt > 0) {
        const record = await getUploadById(id);
        currentOssUploadId = record?.ossUploadId;
      }

      // 调用 Rust 后端上传
      try {
        await invoke('upload_to_oss', {
          uploadId: id,
          filePath,
          bucket,
          object,
          endpoint: token.endpoint,
          accessKeyId: token.AccessKeyId,
          accessKeySecret: token.AccessKeySecret,
          securityToken: token.SecurityToken,
          callback,
          callbackVar,
          ossUploadId: currentOssUploadId || null,
          tokenExpirationMs,
        });

        // 上传成功，进度事件中已处理了完成状态
        return;
      } catch (e) {
        const errMsg = String(e);

        if (errMsg === 'token_expired' && tokenAttempt < MAX_TOKEN_REFRESH) {
          // STS 凭证过期，刷新凭证后重试（ossUploadId 保持不变，已上传的分片不会丢失）
          console.warn(
            `STS 凭证过期，正在刷新凭证重试 (${tokenAttempt + 1}/${MAX_TOKEN_REFRESH}): ${id}`,
          );
          continue;
        }

        if (errMsg === 'upload_cancelled') {
          await updateUpload(id, {
            status: 'cancelled',
            uploadSpeed: 0,
          });
        } else {
          await updateUpload(id, {
            status: 'error',
            errorMessage: `OSS 上传失败: ${errMsg}`,
            uploadSpeed: 0,
          });
        }
        throw e;
      }
    }
  };

  /** 处理二次认证 */
  const handleSignCheck = async (
    id: string,
    fileName: string,
    filePath: string,
    fileSize: number,
    target: string,
    sha1: string,
    initData: { sign_key?: string; sign_check?: string; pick_code: string },
  ) => {
    if (!initData.sign_check || !initData.sign_key) return;

    // 解析 sign_check 范围 "start-end"
    const [startStr, endStr] = initData.sign_check.split('-');
    const start = parseInt(startStr!);
    const end = parseInt(endStr!);

    // 计算指定区间的 SHA1
    const signVal: string = await invoke('compute_partial_sha1', {
      filePath,
      start,
      end,
    });

    // 重新调用上传接口（附带签名信息）
    const res = await uploadInit({
      file_name: fileName,
      file_size: fileSize,
      target,
      fileid: sha1,
      pick_code: initData.pick_code,
      sign_key: initData.sign_key,
      sign_val: signVal,
    });

    const data = res.data;
    if (!data) throw new Error('二次认证失败：无返回数据');

    if (data.status === 2) {
      await updateUpload(id, {
        status: 'complete',
        progress: 100,
        completedAt: Date.now(),
        fileId: data.file_id,
      });
    } else if (data.status === 1 && data.bucket && data.object && data.callback) {
      // 需要继续上传
      await doOssUpload(
        id,
        filePath,
        data.bucket,
        data.object,
        data.callback.callback,
        data.callback.callback_var,
      );
    }
  };

  // ---------- 公开接口 ----------

  /** 上传多个本地文件 (文件选择对话框后调用) */
  const uploadFiles = async (
    files: { path: string; name: string; size: number }[],
    targetCid: string,
  ) => {
    for (const file of files) {
      enqueueFile(file.path, file.name, file.size, targetCid);
    }
    startPolling();
  };

  /** 暂停上传任务 */
  const pauseTask = async (uploadFile: UploadFile) => {
    if (uploadFile.isFolder) {
      // 从队列中移除该文件夹尚未开始的子任务
      uploadQueue.value = uploadQueue.value.filter((q) => q.parentId !== uploadFile.id);

      const children = await getChildUploads(uploadFile.id);
      const active = children.filter(
        (d) => d.status === 'uploading' || d.status === 'pending' || d.status === 'hashing',
      );
      for (const child of active) {
        try {
          await invoke('pause_upload', { uploadId: child.id });
        } catch {
          // 可能任务还未开始OSS上传
        }
        await updateUpload(child.id, { status: 'paused', uploadSpeed: 0 });
      }
      await updateUpload(uploadFile.id, { status: 'paused', uploadSpeed: 0 });
    } else {
      try {
        await invoke('pause_upload', { uploadId: uploadFile.id });
      } catch {
        // 可能任务还未开始OSS上传
      }
      await updateUpload(uploadFile.id, { status: 'paused', uploadSpeed: 0 });
    }
    await refreshDisplayList();
  };

  /** 恢复上传任务 */
  const resumeTask = async (uploadFile: UploadFile) => {
    if (uploadFile.isFolder) {
      const children = await getChildUploads(uploadFile.id);
      const paused = children.filter((d) => d.status === 'paused');
      for (const child of paused) {
        try {
          await invoke('resume_upload', { uploadId: child.id });
          await updateUpload(child.id, { status: 'uploading' });
        } catch {
          // 任务需要重新开始，复用已有 DB 记录
          // 携带已保存的 sha1/pickCode，走断点续传流程
          await updateUpload(child.id, {
            status: 'pending',
            progress: 0,
            uploadSpeed: 0,
            errorMessage: undefined,
            uploadedSize: 0,
          });
          uploadQueue.value.push({
            filePath: child.filePath,
            fileName: child.fileName,
            fileSize: child.fileSize,
            targetCid: child.targetCid,
            retryCount: 0,
            parentId: uploadFile.id,
            dbId: child.id,
            sha1: child.sha1,
            preSha1: child.preSha1,
            pickCode: child.pickCode,
            ossUploadId: child.ossUploadId,
          });
        }
      }
      await updateUpload(uploadFile.id, { status: 'uploading' });
      processQueue();
    } else {
      try {
        await invoke('resume_upload', { uploadId: uploadFile.id });
        await updateUpload(uploadFile.id, { status: 'uploading' });
      } catch {
        // 如果 Rust 端没有该任务，需要重新排队，复用已有 DB 记录
        // 携带已保存的 sha1/pickCode，走断点续传流程避免重复计算
        await updateUpload(uploadFile.id, {
          status: 'pending',
          progress: 0,
          uploadSpeed: 0,
          errorMessage: undefined,
          uploadedSize: 0,
        });
        uploadQueue.value.push({
          filePath: uploadFile.filePath,
          fileName: uploadFile.fileName,
          fileSize: uploadFile.fileSize,
          targetCid: uploadFile.targetCid,
          retryCount: 0,
          parentId: uploadFile.parentId,
          dbId: uploadFile.id,
          sha1: uploadFile.sha1,
          preSha1: uploadFile.preSha1,
          pickCode: uploadFile.pickCode,
          ossUploadId: uploadFile.ossUploadId,
        });
        processQueue();
      }
    }
    startPolling();
    await refreshDisplayList();
  };

  /** 重试失败的上传任务 */
  const retryTask = async (uploadFile: UploadFile) => {
    if (uploadFile.isFolder) {
      const children = await getChildUploads(uploadFile.id);
      const failed = children.filter((d) => d.status === 'error');
      for (const child of failed) {
        // 复用已有 DB 记录，重置状态
        await updateUpload(child.id, {
          status: 'pending',
          progress: 0,
          uploadSpeed: 0,
          errorMessage: undefined,
          uploadedSize: 0,
        });
        uploadQueue.value.push({
          filePath: child.filePath,
          fileName: child.fileName,
          fileSize: child.fileSize,
          targetCid: child.targetCid,
          retryCount: 0,
          parentId: uploadFile.id,
          dbId: child.id,
          sha1: child.sha1,
          preSha1: child.preSha1,
          pickCode: child.pickCode,
          ossUploadId: child.ossUploadId,
        });
      }
      await updateUpload(uploadFile.id, {
        status: 'uploading',
        failedFiles: 0,
        errorMessage: undefined,
        completedAt: undefined,
      });
    } else {
      // 复用已有 DB 记录，重置状态
      await updateUpload(uploadFile.id, {
        status: 'pending',
        progress: 0,
        uploadSpeed: 0,
        errorMessage: undefined,
        uploadedSize: 0,
      });
      uploadQueue.value.push({
        filePath: uploadFile.filePath,
        fileName: uploadFile.fileName,
        fileSize: uploadFile.fileSize,
        targetCid: uploadFile.targetCid,
        retryCount: 0,
        parentId: uploadFile.parentId,
        dbId: uploadFile.id,
        sha1: uploadFile.sha1,
        preSha1: uploadFile.preSha1,
        pickCode: uploadFile.pickCode,
        ossUploadId: uploadFile.ossUploadId,
      });
    }

    processQueue();
    startPolling();
    await refreshDisplayList();
  };

  /** 移除上传任务 */
  const removeTask = async (uploadFile: UploadFile) => {
    if (uploadFile.isFolder) {
      // 从队列中移除该文件夹的子任务
      uploadQueue.value = uploadQueue.value.filter((q) => q.parentId !== uploadFile.id);

      const children = await getChildUploads(uploadFile.id);
      for (const child of children) {
        if (child.status === 'uploading') {
          try {
            await invoke('cancel_upload', { uploadId: child.id });
          } catch {
            // 忽略
          }
        }
      }
      await deleteChildUploads(uploadFile.id);
      await deleteUpload(uploadFile.id);
    } else {
      if (uploadFile.status === 'uploading') {
        try {
          await invoke('cancel_upload', { uploadId: uploadFile.id });
        } catch {
          // 忽略
        }
      }
      await deleteUpload(uploadFile.id);
    }
    await refreshDisplayList();
  };

  /** 清除所有已完成的上传记录 */
  const clearFinished = async () => {
    await deleteFinishedUploads();
    await refreshDisplayList();
  };

  // ---------- 计算属性 ----------

  const queueStatus = computed(() => ({
    queueLength: uploadQueue.value.length,
    isProcessing: isProcessing.value,
    isPolling: isPolling.value,
  }));

  const uploadStats = computed(() => {
    const list = displayList.value;
    const active = list.filter(
      (d) => d.status === 'uploading' || d.status === 'hashing' || d.status === 'pending',
    );
    const totalSpeed = active.reduce((sum, d) => sum + (d.uploadSpeed || 0), 0);
    return {
      activeCount: active.length,
      totalSpeed,
      completed: list.filter((d) => d.status === 'complete').length,
      failed: list.filter((d) => d.status === 'error').length,
      paused: list.filter((d) => d.status === 'paused').length,
      total: list.length,
    };
  });

  // ---------- 初始化 ----------

  let initialized = false;

  const init = async () => {
    if (initialized) return;
    initialized = true;

    await setupProgressListener();

    // 将未完成的任务标记为暂停（因为 Rust 端的上传进程不会在重启后保留）
    const allItems = await getAllUploads();
    const activeTasks = allItems.filter(
      (d) =>
        !d.isFolder &&
        (d.status === 'uploading' || d.status === 'hashing' || d.status === 'pending'),
    );
    for (const task of activeTasks) {
      await updateUpload(task.id, { status: 'paused', uploadSpeed: 0 });
    }

    // 更新文件夹状态
    const activeFolders = allItems.filter(
      (d) =>
        d.isFolder && d.status !== 'complete' && d.status !== 'error' && d.status !== 'cancelled',
    );
    for (const folder of activeFolders) {
      await updateUpload(folder.id, { status: 'paused', uploadSpeed: 0 });
    }

    await refreshDisplayList();
  };

  return {
    init,
    displayList,
    uploadFile,
    uploadFiles,
    uploadFolder,
    pauseTask,
    resumeTask,
    retryTask,
    removeTask,
    clearFinished,
    startPolling,
    stopPolling,
    queueStatus,
    uploadStats,
  };
});
