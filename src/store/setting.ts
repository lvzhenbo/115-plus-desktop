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
}

export const useSettingStore = defineStore(
  'setting',
  () => {
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
      downloadList: [] as Array<DownLoadFile>,
    });

    return {
      videoPlayerSetting,
      cloudDownloadSetting,
      downloadSetting,
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
