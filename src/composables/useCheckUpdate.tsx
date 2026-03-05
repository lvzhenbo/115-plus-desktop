import type { Update } from '@tauri-apps/plugin-updater';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { useSettingStore } from '@/store/setting';
import { filesize } from 'filesize';
import { NButton } from 'naive-ui';

const isChecking = ref(false);

export const useCheckUpdate = () => {
  const message = useMessage();
  const dialog = useDialog();
  const notification = useNotification();
  const settingStore = useSettingStore();

  async function confirmAndInstall(update: Update) {
    const confirmed = await new Promise<boolean>((resolve) => {
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

    if (!confirmed) return;

    const msgReactive = message.loading('正在下载更新...', { duration: 0 });

    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case 'Started':
          msgReactive.content = `正在下载更新 (${filesize(event.data.contentLength ?? 0)})...`;
          break;
        case 'Finished':
          msgReactive.destroy();
          message.success('下载完成，即将安装更新...');
          break;
      }
    });

    await relaunch();
  }

  async function checkForUpdate(options?: { silent?: boolean }) {
    const silent = options?.silent ?? false;
    const proxy = settingStore.generalSetting.updateProxy || undefined;

    if (isChecking.value) return;
    isChecking.value = true;

    try {
      const update = await check({ proxy });
      console.log(update);

      if (!update) {
        if (!silent) {
          message.success('当前已是最新版本');
        }
        return;
      }

      if (silent) {
        const n = notification.info({
          title: `发现新版本 v${update.version}`,
          content: update.body || '暂无更新说明',
          action: () => (
            <NButton
              type="primary"
              text
              onClick={() => {
                n.destroy();
                confirmAndInstall(update);
              }}
            >
              点击更新
            </NButton>
          ),
        });
        return;
      }

      await confirmAndInstall(update);
    } catch (error) {
      console.error(error);

      if (!silent) {
        message.error(`检查更新失败: ${error instanceof Error ? error.message : String(error)}`);
      }
    } finally {
      isChecking.value = false;
    }
  }

  return { isChecking, checkForUpdate };
};
