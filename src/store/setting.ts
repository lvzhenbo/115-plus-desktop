import store from '.';

export type AppLogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error';

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
      /** 应用日志级别 */
      logLevel: 'info' as AppLogLevel,
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
      downloadPath: '',
      /** 下载失败最大重试次数 */
      maxRetry: 5,
      /** 并行下载任务数 */
      maxConcurrent: 5,
      /** 文件拆分的分片数 */
      split: 16,
      /** 全局最大并发连接数 */
      maxGlobalConnections: 16,
      /** 是否启用下载限速 */
      speedLimitEnabled: false,
      /** 限速数值（用户输入值，需结合 speedLimitUnit 换算） */
      speedLimitValue: 10,
      /** 限速单位 */
      speedLimitUnit: 'MB/s' as 'KB/s' | 'MB/s',
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
