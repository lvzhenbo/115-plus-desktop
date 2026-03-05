import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { useMessage, useDialog, useNotification } from '@/composables/useDiscreteApi';
import { useSettingStoreWithOut } from '@/store/setting';

const isChecking = ref(false);

export function useCheckUpdate() {
  return { isChecking, checkForUpdate };
}

async function checkForUpdate(options?: { silent?: boolean }) {
  const silent = options?.silent ?? false;
  const message = useMessage();
  const dialog = useDialog();
  const notification = useNotification();
  const settingStore = useSettingStoreWithOut();
  const proxy = settingStore.generalSetting.updateProxy || undefined;

  if (isChecking.value) return;
  isChecking.value = true;

  try {
    const update = await check({ proxy });

    if (!update) {
      if (!silent) {
        message.success('当前已是最新版本');
      }
      return;
    }

    const confirmUpdate = () => {
      return new Promise<boolean>((resolve) => {
        dialog.info({
          title: `发现新版本 v${update.version}`,
          content: update.body || '暂无更新说明',
          positiveText: '立即更新',
          negativeText: '稍后再说',
          onPositiveClick: () => resolve(true),
          onNegativeClick: () => resolve(false),
          onClose: () => resolve(false),
          onMaskClick: () => resolve(false),
        });
      });
    };

    if (silent) {
      notification.info({
        title: '发现新版本',
        content: `v${update.version} 可用，点击前往关于页面更新`,
        duration: 5000,
      });
      return;
    }

    const confirmed = await confirmUpdate();
    if (!confirmed) return;

    const msgReactive = message.loading('正在下载更新...', { duration: 0 });

    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case 'Started':
          msgReactive.content = `正在下载更新 (${formatBytes(event.data.contentLength ?? 0)})...`;
          break;
        case 'Finished':
          msgReactive.destroy();
          message.success('下载完成，即将安装更新...');
          break;
      }
    });

    await relaunch();
  } catch (error) {
    console.error(error);

    if (!silent) {
      message.error(`检查更新失败: ${error instanceof Error ? error.message : String(error)}`);
    }
  } finally {
    isChecking.value = false;
  }
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}
