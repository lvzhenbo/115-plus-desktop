import Database from '@tauri-apps/plugin-sql';
import type { DownLoadFile } from '@/store/setting';

let db: Database | null = null;

/**
 * 获取数据库实例（单例）
 */
export const getDb = async (): Promise<Database> => {
  if (!db) {
    db = await Database.load('sqlite:downloads.db');
  }
  return db;
};

/** 数据库行类型 */
interface DownloadRow {
  gid: string;
  fid: string;
  name: string;
  pick_code: string;
  size: number;
  status: string;
  progress: number;
  path: string | null;
  download_speed: number;
  eta: number | null;
  error_message: string | null;
  error_code: string | null;
  created_at: number | null;
  completed_at: number | null;
  is_folder: number;
  is_collecting: number;
  parent_gid: string | null;
  total_files: number | null;
  completed_files: number | null;
  failed_files: number | null;
}

/** 数据库行 → DownLoadFile */
const rowToDownloadFile = (row: DownloadRow): DownLoadFile => ({
  gid: row.gid,
  fid: row.fid,
  name: row.name,
  pickCode: row.pick_code,
  size: row.size,
  status: row.status as DownLoadFile['status'],
  progress: row.progress,
  path: row.path ?? undefined,
  downloadSpeed: row.download_speed,
  eta: row.eta ?? undefined,
  errorMessage: row.error_message ?? undefined,
  errorCode: row.error_code ?? undefined,
  createdAt: row.created_at ?? undefined,
  completedAt: row.completed_at ?? undefined,
  isFolder: row.is_folder === 1,
  isCollecting: row.is_collecting === 1,
  parentGid: row.parent_gid ?? undefined,
  totalFiles: row.total_files ?? undefined,
  completedFiles: row.completed_files ?? undefined,
  failedFiles: row.failed_files ?? undefined,
});

// ==================== CRUD 操作 ====================

/**
 * 插入一条下载记录
 */
export const insertDownload = async (file: DownLoadFile): Promise<void> => {
  const d = await getDb();
  await d.execute(
    `INSERT OR REPLACE INTO downloads (
      gid, fid, name, pick_code, size, status, progress, path,
      download_speed, eta, error_message, error_code,
      created_at, completed_at, is_folder, is_collecting,
      parent_gid, total_files, completed_files, failed_files
    ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)`,
    [
      file.gid,
      file.fid,
      file.name,
      file.pickCode,
      file.size,
      file.status ?? 'active',
      file.progress ?? 0,
      file.path ?? null,
      file.downloadSpeed ?? 0,
      file.eta ?? null,
      file.errorMessage ?? null,
      file.errorCode ?? null,
      file.createdAt ?? null,
      file.completedAt ?? null,
      file.isFolder ? 1 : 0,
      file.isCollecting ? 1 : 0,
      file.parentGid ?? null,
      file.totalFiles ?? null,
      file.completedFiles ?? null,
      file.failedFiles ?? null,
    ],
  );
};

/**
 * 批量插入下载记录
 */
export const batchInsertDownloads = async (files: DownLoadFile[]): Promise<void> => {
  const d = await getDb();
  for (const file of files) {
    await d.execute(
      `INSERT OR REPLACE INTO downloads (
        gid, fid, name, pick_code, size, status, progress, path,
        download_speed, eta, error_message, error_code,
        created_at, completed_at, is_folder, is_collecting,
        parent_gid, total_files, completed_files, failed_files
      ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)`,
      [
        file.gid,
        file.fid,
        file.name,
        file.pickCode,
        file.size,
        file.status ?? 'active',
        file.progress ?? 0,
        file.path ?? null,
        file.downloadSpeed ?? 0,
        file.eta ?? null,
        file.errorMessage ?? null,
        file.errorCode ?? null,
        file.createdAt ?? null,
        file.completedAt ?? null,
        file.isFolder ? 1 : 0,
        file.isCollecting ? 1 : 0,
        file.parentGid ?? null,
        file.totalFiles ?? null,
        file.completedFiles ?? null,
        file.failedFiles ?? null,
      ],
    );
  }
};

/**
 * 更新下载记录的部分字段
 */
export const updateDownload = async (
  gid: string,
  updates: Partial<DownLoadFile>,
): Promise<void> => {
  const d = await getDb();
  const setClauses: string[] = [];
  const values: unknown[] = [];
  let idx = 1;

  const fieldMap: Record<string, string> = {
    fid: 'fid',
    name: 'name',
    pickCode: 'pick_code',
    size: 'size',
    status: 'status',
    progress: 'progress',
    path: 'path',
    downloadSpeed: 'download_speed',
    eta: 'eta',
    errorMessage: 'error_message',
    errorCode: 'error_code',
    createdAt: 'created_at',
    completedAt: 'completed_at',
    isFolder: 'is_folder',
    isCollecting: 'is_collecting',
    parentGid: 'parent_gid',
    totalFiles: 'total_files',
    completedFiles: 'completed_files',
    failedFiles: 'failed_files',
  };

  for (const [key, value] of Object.entries(updates)) {
    if (key === 'gid') continue; // gid 是主键，不更新
    const col = fieldMap[key];
    if (!col) continue;

    setClauses.push(`${col} = $${idx}`);
    if (key === 'isFolder' || key === 'isCollecting') {
      values.push(value ? 1 : 0);
    } else {
      values.push(value ?? null);
    }
    idx++;
  }

  if (setClauses.length === 0) return;

  values.push(gid);
  await d.execute(`UPDATE downloads SET ${setClauses.join(', ')} WHERE gid = $${idx}`, values);
};

/**
 * 通过 gid 删除下载记录
 */
export const deleteDownload = async (gid: string): Promise<void> => {
  const d = await getDb();
  await d.execute('DELETE FROM downloads WHERE gid = $1', [gid]);
};

/**
 * 删除某个文件夹的所有子任务
 */
export const deleteChildDownloads = async (parentGid: string): Promise<void> => {
  const d = await getDb();
  await d.execute('DELETE FROM downloads WHERE parent_gid = $1', [parentGid]);
};

/**
 * 删除所有已完成/已失败的记录（包括其子任务）
 */
export const deleteFinishedDownloads = async (): Promise<void> => {
  const d = await getDb();
  // 先获取要删除的文件夹 gid
  const folders = await d.select<{ gid: string }[]>(
    "SELECT gid FROM downloads WHERE is_folder = 1 AND status IN ('complete', 'error', 'removed')",
  );
  const folderGids = folders.map((f) => f.gid);

  // 删除这些文件夹的子任务
  if (folderGids.length > 0) {
    const placeholders = folderGids.map((_, i) => `$${i + 1}`).join(',');
    await d.execute(`DELETE FROM downloads WHERE parent_gid IN (${placeholders})`, folderGids);
  }

  // 删除顶层已完成/失败的记录
  await d.execute(
    "DELETE FROM downloads WHERE parent_gid IS NULL AND status IN ('complete', 'error', 'removed')",
  );
};

/**
 * 查询所有顶层下载记录（不含子任务）
 */
export const getTopLevelDownloads = async (): Promise<DownLoadFile[]> => {
  const d = await getDb();
  const rows = await d.select<DownloadRow[]>(
    'SELECT * FROM downloads WHERE parent_gid IS NULL ORDER BY created_at DESC',
  );
  return rows.map(rowToDownloadFile);
};

/**
 * 查询所有下载记录
 */
export const getAllDownloads = async (): Promise<DownLoadFile[]> => {
  const d = await getDb();
  const rows = await d.select<DownloadRow[]>('SELECT * FROM downloads ORDER BY created_at DESC');
  return rows.map(rowToDownloadFile);
};

/**
 * 通过 gid 查询单条记录
 */
export const getDownloadByGid = async (gid: string): Promise<DownLoadFile | null> => {
  const d = await getDb();
  const rows = await d.select<DownloadRow[]>('SELECT * FROM downloads WHERE gid = $1', [gid]);
  return rows.length > 0 ? rowToDownloadFile(rows[0]!) : null;
};

/**
 * 查询某文件夹的所有子任务
 */
export const getChildDownloads = async (parentGid: string): Promise<DownLoadFile[]> => {
  const d = await getDb();
  const rows = await d.select<DownloadRow[]>(
    'SELECT * FROM downloads WHERE parent_gid = $1 ORDER BY created_at ASC',
    [parentGid],
  );
  return rows.map(rowToDownloadFile);
};

/**
 * 查询所有需要恢复的任务（未完成的非文件夹任务）
 */
export const getIncompleteDownloads = async (): Promise<DownLoadFile[]> => {
  const d = await getDb();
  const rows = await d.select<DownloadRow[]>(
    `SELECT * FROM downloads
     WHERE is_folder = 0
       AND gid NOT LIKE 'failed-%'
       AND gid NOT LIKE 'folder-%'
       AND status NOT IN ('complete', 'error', 'removed')
     ORDER BY created_at ASC`,
  );
  return rows.map(rowToDownloadFile);
};

/**
 * 查询活跃的 aria2 任务 gid 列表（用于状态轮询）
 */
export const getActiveGids = async (): Promise<string[]> => {
  const d = await getDb();
  const rows = await d.select<{ gid: string }[]>(
    `SELECT gid FROM downloads
     WHERE is_folder = 0
       AND gid NOT LIKE 'failed-%'
       AND gid NOT LIKE 'folder-%'
       AND status IN ('active', 'waiting', 'paused')`,
  );
  return rows.map((r) => r.gid);
};

/**
 * 查询是否有活跃任务
 */
export const hasActiveDownloads = async (): Promise<boolean> => {
  const d = await getDb();
  const rows = await d.select<{ cnt: number }[]>(
    `SELECT COUNT(*) as cnt FROM downloads
     WHERE (is_folder = 0 AND gid NOT LIKE 'failed-%' AND status IN ('active', 'waiting', 'paused'))
        OR (is_folder = 1 AND is_collecting = 1)`,
  );
  return (rows[0]?.cnt ?? 0) > 0;
};

/**
 * 获取下载统计（仅顶层任务）
 */
export const getDownloadStats = async () => {
  const d = await getDb();
  const rows = await d.select<{ status: string; cnt: number; total_speed: number }[]>(
    `SELECT status, COUNT(*) as cnt, SUM(download_speed) as total_speed
     FROM downloads
     WHERE parent_gid IS NULL
     GROUP BY status`,
  );

  const stats = {
    activeCount: 0,
    totalSpeed: 0,
    completed: 0,
    failed: 0,
    paused: 0,
    waiting: 0,
    total: 0,
  };

  for (const row of rows) {
    stats.total += row.cnt;
    switch (row.status) {
      case 'active':
        stats.activeCount = row.cnt;
        stats.totalSpeed = row.total_speed || 0;
        break;
      case 'complete':
        stats.completed = row.cnt;
        break;
      case 'error':
        stats.failed = row.cnt;
        break;
      case 'paused':
        stats.paused = row.cnt;
        break;
      case 'waiting':
        stats.waiting = row.cnt;
        break;
    }
  }

  return stats;
};
