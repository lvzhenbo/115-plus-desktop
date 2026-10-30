import { fileList, fileSearch } from '@/api/file';
import type {
  FileListRequestParams,
  FileSearchRequestParams,
  MyFile,
  SearchFile,
} from '@/api/types/file';

export type PreviewFile = Pick<MyFile, 'fid' | 'fn' | 'pid' | 'pc' | 'ico' | 'uo'> & {
  /** 搜索结果未提供视频标记，需要打开时补齐文件信息。 */
  isv?: number;
};

export type MediaSource =
  | { kind: 'files'; params: FileListRequestParams }
  | { kind: 'search'; params: FileSearchRequestParams };

export interface VideoPlayback {
  file: PreviewFile;
  source: MediaSource;
}

export function searchPreviewFile(file: SearchFile, type?: number): PreviewFile {
  return {
    fid: file.file_id,
    fn: file.file_name,
    pid: file.parent_id,
    pc: file.pick_code,
    ico: file.ico || '',
    isv: type === 4 ? 1 : undefined,
  };
}

/** 只补齐当前要打开的文件，避免为整页搜索结果逐项请求详情。 */
export async function resolvePreviewFile(file: PreviewFile): Promise<PreviewFile> {
  if (file.isv !== undefined) return file;
  let offset = 0;
  while (true) {
    const res = await fileList({
      cid: file.pid,
      cur: 1,
      show_dir: 0,
      suffix: file.ico || undefined,
      limit: 1150,
      offset,
    });
    const found = res.data.find((item) => item.fid === file.fid);
    if (found) return found;
    offset += res.data.length;
    if (!res.data.length || offset >= res.count) {
      throw new Error('文件不存在或已被删除');
    }
  }
}

/** 媒体单独分页，不能直接复用混合文件列表的页码。查询条件在打开时固定。 */
export function createMediaList(source: MediaSource, type: 2 | 4) {
  const params = { ...source.params };
  const items: PreviewFile[] = [];
  const ids = new Set<string>();
  let offset = 0;
  let total = Infinity;
  let active = true;
  let pending: Promise<void> | undefined;
  const hasMore = () => active && offset < total;

  function loadMore(): Promise<void> {
    if (pending) return pending;
    if (!hasMore()) return Promise.resolve();
    pending = (async () => {
      let page: PreviewFile[];
      let count: number;
      if (source.kind === 'search') {
        const res = await fileSearch({
          ...(params as FileSearchRequestParams),
          type,
          fc: 2,
          offset,
          limit: Math.min(1150, 10000 - offset),
        });
        page = res.data.map((file) => searchPreviewFile(file, type));
        count = Math.min(res.count, 10000);
      } else {
        const res = await fileList({
          ...params,
          type,
          nf: 0,
          show_dir: 0,
          offset,
          limit: 1150,
        });
        page = res.data;
        count = res.count;
      }
      if (!active) return;
      offset += page.length;
      total = page.length ? count : offset;
      for (const file of page) {
        if (type === 2 && source.kind === 'files' && !file.uo) continue;
        if (!ids.has(file.fid)) {
          ids.add(file.fid);
          items.push(file);
        }
      }
    })().finally(() => {
      pending = undefined;
    });
    return pending;
  }

  return {
    items,
    hasMore,
    loadMore,
    cancel: () => {
      active = false;
    },
    async locate(file: PreviewFile) {
      while (!ids.has(file.fid) && hasMore()) await loadMore();
      if (!active) return -1;
      const index = items.findIndex((item) => item.fid === file.fid);
      if (index < 0) throw new Error('文件不在当前列表中，请刷新后重试');
      items[index] = { ...items[index]!, ...file };
      return index;
    },
  };
}
