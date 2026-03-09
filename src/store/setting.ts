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
      /** 启动时自动检查更新 */
      autoCheckUpdate: true,
      /** 更新请求代理地址（留空不使用代理） */
      updateProxy: '',
      /** 115接口速率限制（每秒请求数，0为不限制） */
      apiRateLimit: 2,
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

    const subtitleStyleSetting = ref({
      /** 是否默认开启字幕 */
      defaultEnabled: true,
      /** 字体大小 (px) */
      fontSize: 22,
      /** 字体颜色 */
      fontColor: '#FFFFFF',
      /** 是否加粗 */
      fontBold: false,
      /** 描边颜色 */
      strokeColor: '#000000',
      /** 描边宽度 (px) */
      strokeWidth: 1,
      /** 背景颜色 (hex8 含透明度) */
      backgroundColor: '#00000099',
      /** 距底部偏移 (px) */
      bottomOffset: 64,
    });

    return {
      generalSetting,
      videoPlayerSetting,
      cloudDownloadSetting,
      downloadSetting,
      uploadSetting,
      subtitleStyleSetting,
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
