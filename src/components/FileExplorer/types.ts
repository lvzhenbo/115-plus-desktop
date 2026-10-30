import type { MyFile, SortField } from '@/api/types/file';
import fileIconPaths from '@/assets/file-icons/map.json';

/** 视图模式 */
export type ViewMode = 'grid' | 'list';

/** 排序方向 */
export type SortDirection = 'asc' | 'desc';

/** 排序配置 */
export interface SortConfig {
  field: SortField;
  direction: SortDirection;
}

/** 工具栏可控制的功能项 */
export type ToolbarAction =
  | 'up'
  | 'refresh'
  | 'newFolder'
  | 'upload'
  | 'download'
  | 'copy'
  | 'move'
  | 'rename'
  | 'delete'
  | 'filter'
  | 'viewToggle';

/** 右键菜单可控制的功能项 */
export type ContextMenuAction =
  | 'open'
  | 'reload'
  | 'download'
  | 'uploadFile'
  | 'copy'
  | 'move'
  | 'rename'
  | 'batchRename'
  | 'detail'
  | 'delete'
  | 'toggleStar';

/** 列表模式可展示的列（name 始终展示） */
export type ListColumn = 'size' | 'type' | 'createTime' | 'modifyTime';

/** 文件行展示所需的数据，不要求调用方补齐完整接口字段。 */
export type FileItemData = Pick<MyFile, 'fid' | 'fn' | 'fc' | 'ico'> &
  Partial<Pick<MyFile, 'fs' | 'uppt' | 'uet' | 'thumb' | 'ism' | 'isv' | 'def2'>>;

const iconPaths: Record<string, string> = fileIconPaths;
const icons = import.meta.glob<string>('/src/assets/file-icons/**/*.svg', {
  eager: true,
  query: '?url',
  import: 'default',
});

const videoQualities: Record<number, string> = {
  1: 'video-sd',
  2: 'video-hd',
  3: 'video-fhd',
  4: 'video-1080p',
  5: 'video-4k',
  100: 'video-origin',
};

/** 文件夹按接口类型识别；视频优先使用官方清晰度标记，其余按后缀映射。 */
export function getFileIcon(
  file: Pick<MyFile, 'fc' | 'ico'> & Partial<Pick<MyFile, 'isv' | 'def2'>>,
): string {
  const quality = file.isv === 1 ? videoQualities[file.def2 ?? 0] : undefined;
  const type = file.fc === '0' ? 'folder' : quality || file.ico?.toLowerCase() || '';
  const path = iconPaths[type];
  return (
    icons[`/src/assets/file-icons/${path}`] || icons['/src/assets/file-icons/other/unknown.svg']!
  );
}
