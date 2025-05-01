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
    persist: true,
  },
);

export function useSettingStoreWithOut() {
  return useSettingStore(store);
}
