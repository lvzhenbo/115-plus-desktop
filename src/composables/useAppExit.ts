/**
 * 应用退出桥接层。
 *
 * 统一处理托盘"退出"菜单、Cmd+Q / Alt+F4 等退出场景：
 * - 检查是否有活跃的下载/上传任务
 * - 根据用户设置决定是否弹出确认对话框
 * - 暂停所有传输任务后退出进程
 *
 * 该 composable 在 App.vue 中初始化，确保登录页、主页等所有页面下都能响应退出事件。
 */

import { listen } from '@tauri-apps/api/event';
import { exit } from '@tauri-apps/plugin-process';
import { ask } from '@tauri-apps/plugin-dialog';
import { useDownloadManager } from './useDownloadManager';
import { useUploadManager } from './useUploadManager';
import { useSettingStore } from '@/store/setting';

export const useAppExit = createSharedComposable(() => {
  const { hasActiveDownloads, pauseAllTasks: pauseAllDownloads } = useDownloadManager();
  const { hasActiveUploads, pauseAllTasks: pauseAllUploads } = useUploadManager();
  const settingStore = useSettingStore();

  let unlisten: (() => void) | null = null;

  /** 暂停所有传输任务并退出应用 */
  const pauseAndClose = async () => {
    await Promise.all([pauseAllDownloads(), pauseAllUploads()]);
    await exit(0);
  };

  /** 初始化托盘退出事件监听（幂等，重复调用安全） */
  const init = async () => {
    if (unlisten) return;

    unlisten = await listen('tray-quit', async () => {
      const active = hasActiveDownloads.value || hasActiveUploads.value;

      if (!active) {
        await exit(0);
        return;
      }

      if (settingStore.generalSetting.skipExitConfirm) {
        await pauseAndClose();
        return;
      }

      const confirmed = await ask('当前有正在进行的传输任务，确定退出？', {
        title: '提示',
        kind: 'warning',
        okLabel: '确定',
        cancelLabel: '取消',
      });

      if (confirmed) {
        await pauseAndClose();
      }
    });
  };

  return { init, pauseAndClose };
});
