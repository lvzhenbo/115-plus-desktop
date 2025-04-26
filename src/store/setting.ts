import store from '.';

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
