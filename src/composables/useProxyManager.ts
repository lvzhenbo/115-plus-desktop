import { invoke } from '@tauri-apps/api/core';
import { useSettingStore } from '@/store/setting';

// 代理设置的 Rust 侧同步桥接层。
//
// 前端只维护 `proxySetting` 原始配置；具体“功能代理 → 统一代理 → 跟随系统 / 直连”
// 的解析由 Rust `ProxyState` 统一完成，这里负责在启动与设置变化时把配置整体同步过去。

export const useProxyManager = createSharedComposable(() => {
  const settingStore = useSettingStore();

  let initPromise: Promise<void> | null = null;
  let stopHandle: (() => void) | null = null;

  const syncProxyConfig = async () => {
    const { followSystemProxy, unifiedProxy, updateProxy, downloadProxy, uploadProxy } =
      settingStore.proxySetting;

    await invoke('proxy_set_config', {
      config: {
        followSystemProxy,
        unifiedProxy,
        updateProxy,
        downloadProxy,
        uploadProxy,
      },
    });
  };

  // 设置同步只挂一套 watcher，避免 shared composable 重复注册。
  const setupSettingSync = () => {
    if (stopHandle) return;

    stopHandle = watch(
      () => [
        settingStore.proxySetting.followSystemProxy,
        settingStore.proxySetting.unifiedProxy,
        settingStore.proxySetting.updateProxy,
        settingStore.proxySetting.downloadProxy,
        settingStore.proxySetting.uploadProxy,
      ],
      () => {
        void syncProxyConfig().catch((error) => {
          console.error('同步代理设置失败:', error);
        });
      },
    );
  };

  const dispose = () => {
    initPromise = null;
    stopHandle?.();
    stopHandle = null;
  };

  tryOnScopeDispose(dispose);

  const init = async () => {
    if (!initPromise) {
      initPromise = (async () => {
        // 先挂 watcher 再全量同步，避免持久化数据在两者之间完成加载时被漏掉。
        setupSettingSync();
        await syncProxyConfig();
      })().catch((error) => {
        initPromise = null;
        throw error;
      });
    }

    await initPromise;
  };

  return { init, dispose, syncProxyConfig };
});
