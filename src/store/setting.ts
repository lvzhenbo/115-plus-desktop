import store from '.';

export interface DownLoadFile {
  fid: string;
  name: string;
  gid: string;
  size: number;
  pickCode: string;
  status?: 'active' | 'waiting' | 'paused' | 'complete' | 'error' | 'removed';
  progress?: number;
  path?: string;
  downloadSpeed?: number;
  /** 预计剩余时间 (秒) */
  eta?: number;
  /** 错误信息 */
  errorMessage?: string;
  /** aria2 错误码 */
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

export const useSettingStore = defineStore(
  'setting',
  () => {
    const generalSetting = ref({
      /**
       * 排序方式
       * - 0 使用记忆排序，自定义排序失效
       * - 1 使用自定义排序，不使用记忆排序
       * - 2 自定义排序，非文件夹置顶
       */
      customOrder: 0 as 0 | 1 | 2,
      /** 关闭窗口时自动暂停任务并关闭，不弹出确认框 */
      skipExitConfirm: false,
    });

    const videoPlayerSetting = ref({
      defaultVolume: 1,
      defaultRate: 1,
      autoPlay: true,
      isHistory: true,
    });

    const cloudDownloadSetting = ref({
      deleteSourceFile: true,
    });

    const downloadSetting = ref({
      aria2Port: 6800,
      downloadPath: '',
      /** 下载失败最大重试次数 */
      maxRetry: 5,
      /** 并行下载任务数 */
      maxConcurrent: 5,
    });

    const uploadSetting = ref({
      /** 上传失败最大重试次数 */
      maxRetry: 3,
      /** 并行上传任务数 */
      maxConcurrent: 5,
    });

    return {
      generalSetting,
      videoPlayerSetting,
      cloudDownloadSetting,
      downloadSetting,
      uploadSetting,
    };
  },
  {
    tauri: {
      saveOnChange: true,
    },
  },
);

export function useSettingStoreWithOut() {
  return useSettingStore(store);
}
