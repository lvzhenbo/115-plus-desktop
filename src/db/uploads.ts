import Database from '@tauri-apps/plugin-sql';

let db: Database | null = null;

/**
 * 获取上传数据库实例（单例）
 */
export const getUploadDb = async (): Promise<Database> => {
  if (!db) {
    db = await Database.load('sqlite:uploads.db');
  }
  return db;
};

/** 上传文件状态 */
export type UploadStatus =
  | 'pending'
  | 'hashing'
  | 'uploading'
  | 'paused'
  | 'complete'
  | 'error'
  | 'cancelled';

/** 上传文件记录 */
export interface UploadFile {
  /** 唯一标识 */
  id: string;
  /** 文件名 */
  fileName: string;
  /** 本地文件路径 */
  filePath: string;
  /** 文件大小 */
  fileSize: number;
  /** 目标文件夹 cid */
  targetCid: string;
  /** 目标路径（显示用） */
  targetPath?: string;
  /** 文件完整 SHA1 */
  sha1?: string;
  /** 文件前 128K SHA1 */
  preSha1?: string;
  /** 115 返回的 pick_code */
  pickCode?: string;
  /** 上传状态 */
  status: UploadStatus;
  /** 上传进度 (0-100) */
  progress: number;
  /** 上传速度 (bytes/s) */
  uploadSpeed: number;
  /** 错误信息 */
  errorMessage?: string;
  /** 创建时间戳 */
  createdAt?: number;
  /** 完成时间戳 */
  completedAt?: number;
  /** 是否为文件夹上传任务 */
  isFolder?: boolean;
  /** 父文件夹任务 ID */
  parentId?: string;
  /** 文件夹内总文件数 */
  totalFiles?: number;
  /** 文件夹内已完成文件数 */
  completedFiles?: number;
  /** 文件夹内失败文件数 */
  failedFiles?: number;
  /** OSS bucket */
  ossBucket?: string;
  /** OSS object key */
  ossObject?: string;
  /** OSS endpoint */
  ossEndpoint?: string;
  /** OSS callback */
  callback?: string;
  /** OSS callback_var */
  callbackVar?: string;
  /** 已上传大小 */
  uploadedSize?: number;
  /** 115 返回的文件 ID（秒传时） */
  fileId?: string;
  /** OSS 分片上传 ID（断点续传用） */
  ossUploadId?: string;
}

/** 数据库行类型 */
interface UploadRow {
  id: string;
  file_name: string;
  file_path: string;
  file_size: number;
  target_cid: string;
  target_path: string | null;
  sha1: string | null;
  pre_sha1: string | null;
  pick_code: string | null;
  status: string;
  progress: number;
  upload_speed: number;
  error_message: string | null;
  created_at: number | null;
  completed_at: number | null;
  is_folder: number;
  parent_id: string | null;
  total_files: number | null;
  completed_files: number | null;
  failed_files: number | null;
  oss_bucket: string | null;
  oss_object: string | null;
  oss_endpoint: string | null;
  callback: string | null;
  callback_var: string | null;
  uploaded_size: number;
  file_id: string | null;
  oss_upload_id: string | null;
}

/** 数据库行 → UploadFile */
const rowToUploadFile = (row: UploadRow): UploadFile => ({
  id: row.id,
  fileName: row.file_name,
  filePath: row.file_path,
  fileSize: row.file_size,
  targetCid: row.target_cid,
  targetPath: row.target_path ?? undefined,
  sha1: row.sha1 ?? undefined,
  preSha1: row.pre_sha1 ?? undefined,
  pickCode: row.pick_code ?? undefined,
  status: row.status as UploadStatus,
  progress: row.progress,
  uploadSpeed: row.upload_speed,
  errorMessage: row.error_message ?? undefined,
  createdAt: row.created_at ?? undefined,
  completedAt: row.completed_at ?? undefined,
  isFolder: row.is_folder === 1,
  parentId: row.parent_id ?? undefined,
  totalFiles: row.total_files ?? undefined,
  completedFiles: row.completed_files ?? undefined,
  failedFiles: row.failed_files ?? undefined,
  ossBucket: row.oss_bucket ?? undefined,
  ossObject: row.oss_object ?? undefined,
  ossEndpoint: row.oss_endpoint ?? undefined,
  callback: row.callback ?? undefined,
  callbackVar: row.callback_var ?? undefined,
  uploadedSize: row.uploaded_size,
  fileId: row.file_id ?? undefined,
  ossUploadId: row.oss_upload_id ?? undefined,
});

// ==================== CRUD 操作 ====================

/**
 * 插入一条上传记录
 */
export const insertUpload = async (file: UploadFile): Promise<void> => {
  const d = await getUploadDb();
  await d.execute(
    `INSERT OR REPLACE INTO uploads (
      id, file_name, file_path, file_size, target_cid, target_path,
      sha1, pre_sha1, pick_code, status, progress, upload_speed,
      error_message, created_at, completed_at, is_folder, parent_id,
      total_files, completed_files, failed_files,
      oss_bucket, oss_object, oss_endpoint, callback, callback_var,
      uploaded_size, file_id, oss_upload_id
    ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28)`,
    [
      file.id,
      file.fileName,
      file.filePath,
      file.fileSize,
      file.targetCid,
      file.targetPath ?? null,
      file.sha1 ?? null,
      file.preSha1 ?? null,
      file.pickCode ?? null,
      file.status ?? 'pending',
      file.progress ?? 0,
      file.uploadSpeed ?? 0,
      file.errorMessage ?? null,
      file.createdAt ?? null,
      file.completedAt ?? null,
      file.isFolder ? 1 : 0,
      file.parentId ?? null,
      file.totalFiles ?? null,
      file.completedFiles ?? null,
      file.failedFiles ?? null,
      file.ossBucket ?? null,
      file.ossObject ?? null,
      file.ossEndpoint ?? null,
      file.callback ?? null,
      file.callbackVar ?? null,
      file.uploadedSize ?? 0,
      file.fileId ?? null,
      file.ossUploadId ?? null,
    ],
  );
};

/**
 * 更新上传记录的部分字段
 */
export const updateUpload = async (id: string, updates: Partial<UploadFile>): Promise<void> => {
  const d = await getUploadDb();
  const setClauses: string[] = [];
  const values: unknown[] = [];
  let idx = 1;

  const fieldMap: Record<string, string> = {
    fileName: 'file_name',
    filePath: 'file_path',
    fileSize: 'file_size',
    targetCid: 'target_cid',
    targetPath: 'target_path',
    sha1: 'sha1',
    preSha1: 'pre_sha1',
    pickCode: 'pick_code',
    status: 'status',
    progress: 'progress',
    uploadSpeed: 'upload_speed',
    errorMessage: 'error_message',
    createdAt: 'created_at',
    completedAt: 'completed_at',
    isFolder: 'is_folder',
    parentId: 'parent_id',
    totalFiles: 'total_files',
    completedFiles: 'completed_files',
    failedFiles: 'failed_files',
    ossBucket: 'oss_bucket',
    ossObject: 'oss_object',
    ossEndpoint: 'oss_endpoint',
    callback: 'callback',
    callbackVar: 'callback_var',
    uploadedSize: 'uploaded_size',
    fileId: 'file_id',
    ossUploadId: 'oss_upload_id',
  };

  for (const [key, value] of Object.entries(updates)) {
    if (key === 'id') continue;
    const col = fieldMap[key];
    if (!col) continue;

    setClauses.push(`${col} = $${idx}`);
    if (key === 'isFolder') {
      values.push(value ? 1 : 0);
    } else {
      values.push(value ?? null);
    }
    idx++;
  }

  if (setClauses.length === 0) return;

  values.push(id);
  await d.execute(`UPDATE uploads SET ${setClauses.join(', ')} WHERE id = $${idx}`, values);
};

/**
 * 删除上传记录
 */
export const deleteUpload = async (id: string): Promise<void> => {
  const d = await getUploadDb();
  await d.execute('DELETE FROM uploads WHERE id = $1', [id]);
};

/**
 * 删除文件夹任务的所有子任务
 */
export const deleteChildUploads = async (parentId: string): Promise<void> => {
  const d = await getUploadDb();
  await d.execute('DELETE FROM uploads WHERE parent_id = $1', [parentId]);
};

/**
 * 删除所有已完成/已失败的上传记录
 */
export const deleteFinishedUploads = async (): Promise<void> => {
  const d = await getUploadDb();
  // 先获取要删除的文件夹 id
  const folders = await d.select<{ id: string }[]>(
    "SELECT id FROM uploads WHERE is_folder = 1 AND status IN ('complete', 'error', 'cancelled')",
  );
  const folderIds = folders.map((f) => f.id);

  if (folderIds.length > 0) {
    const placeholders = folderIds.map((_, i) => `$${i + 1}`).join(',');
    await d.execute(`DELETE FROM uploads WHERE parent_id IN (${placeholders})`, folderIds);
  }

  await d.execute(
    "DELETE FROM uploads WHERE parent_id IS NULL AND status IN ('complete', 'error', 'cancelled')",
  );
};

/**
 * 查询所有顶层上传记录
 */
export const getTopLevelUploads = async (): Promise<UploadFile[]> => {
  const d = await getUploadDb();
  const rows = await d.select<UploadRow[]>(
    'SELECT * FROM uploads WHERE parent_id IS NULL ORDER BY created_at DESC',
  );
  return rows.map(rowToUploadFile);
};

/**
 * 查询所有上传记录
 */
export const getAllUploads = async (): Promise<UploadFile[]> => {
  const d = await getUploadDb();
  const rows = await d.select<UploadRow[]>('SELECT * FROM uploads ORDER BY created_at DESC');
  return rows.map(rowToUploadFile);
};

/**
 * 查询某文件夹的所有子任务
 */
export const getChildUploads = async (parentId: string): Promise<UploadFile[]> => {
  const d = await getUploadDb();
  const rows = await d.select<UploadRow[]>(
    'SELECT * FROM uploads WHERE parent_id = $1 ORDER BY created_at ASC',
    [parentId],
  );
  return rows.map(rowToUploadFile);
};

/**
 * 查询所有未完成的上传任务
 */
export const getIncompleteUploads = async (): Promise<UploadFile[]> => {
  const d = await getUploadDb();
  const rows = await d.select<UploadRow[]>(
    `SELECT * FROM uploads
     WHERE is_folder = 0
       AND status NOT IN ('complete', 'error', 'cancelled')
     ORDER BY created_at ASC`,
  );
  return rows.map(rowToUploadFile);
};

/**
 * 查询活跃的上传任务
 */
export const getActiveUploads = async (): Promise<UploadFile[]> => {
  const d = await getUploadDb();
  const rows = await d.select<UploadRow[]>(
    `SELECT * FROM uploads
     WHERE is_folder = 0
       AND status IN ('pending', 'hashing', 'uploading')
     ORDER BY created_at ASC`,
  );
  return rows.map(rowToUploadFile);
};

/**
 * 通过 id 查询单条记录
 */
export const getUploadById = async (id: string): Promise<UploadFile | null> => {
  const d = await getUploadDb();
  const rows = await d.select<UploadRow[]>('SELECT * FROM uploads WHERE id = $1', [id]);
  return rows.length > 0 ? rowToUploadFile(rows[0]!) : null;
};
