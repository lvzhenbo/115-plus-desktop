import type { MyFile } from '@/api/types/file';

/** 视图模式 */
export type ViewMode = 'grid' | 'list';

/** 排序字段（对应接口 o 参数） */
export type SortField = 'file_name' | 'file_size' | 'user_ptime' | 'user_utime' | 'file_type';

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
  | 'delete'
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
  | 'detail'
  | 'delete';

/** 列表模式可展示的列（name 始终展示） */
export type ListColumn = 'size' | 'type' | 'createTime' | 'modifyTime';

/** 文件图标：按类别分组 → 自动生成扩展名映射 */
const iconCategories: [string, string[]][] = [
  ['📕', ['pdf']],
  ['📄', ['doc', 'docx']],
  ['📊', ['xls', 'xlsx']],
  ['📑', ['ppt', 'pptx']],
  ['📝', ['txt', 'md']],
  ['🖼️', ['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'bmp', 'tiff', 'ico']],
  ['🎬', ['mp4', 'avi', 'mov', 'mkv', 'webm', 'flv', 'wmv', 'rmvb']],
  ['🎵', ['mp3', 'wav', 'flac', 'ogg', 'aac', 'wma', 'ape']],
  ['📦', ['zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz', 'zst']],
  ['⚙️', ['js', 'ts', 'vue', 'json', 'xml', 'yaml', 'yml', 'toml']],
  ['🌐', ['html', 'htm']],
  ['🎨', ['css', 'scss', 'less']],
  ['🐍', ['py']],
  ['☕', ['java', 'kt']],
  ['🔵', ['go']],
  ['🦀', ['rs']],
  ['💻', ['exe', 'msi', 'bat', 'sh']],
  ['📱', ['apk', 'ipa']],
  ['💿', ['iso', 'img', 'dmg']],
  ['🧲', ['torrent']],
];

const iconMap = new Map<string, string>(
  iconCategories.flatMap(([icon, exts]) => exts.map((ext) => [ext, icon] as const)),
);

export function getFileIcon(file: MyFile): string {
  if (file.fc === '0') return '📁';
  const ext = file.ico?.toLowerCase() || '';
  return iconMap.get(ext) || '📄';
}
