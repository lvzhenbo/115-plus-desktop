<script setup lang="ts">
  import { zhCN, dateZhCN, darkTheme, lightTheme } from 'naive-ui';
  import { useDownloadManager } from '@/composables/useDownloadManager';
  import { useUploadManager } from '@/composables/useUploadManager';
  import { useAppExit } from '@/composables/useAppExit';

  const osThemeRef = useOsTheme();
  const theme = computed(() => {
    return osThemeRef.value === 'dark' ? darkTheme : lightTheme;
  });

  // ---- 全局初始化：下载/上传管理器 + 退出事件监听 ----
  // 在 App 层统一初始化，确保登录页等非 Layout 页面也能响应托盘退出等全局事件。

  const { init: initDownloadManager } = useDownloadManager();
  const { init: initUploadManager } = useUploadManager();
  const { init: initAppExit } = useAppExit();

  onMounted(async () => {
    // 初始化下载/上传管理器（幂等，设置事件监听 + 同步后端设置）
    await Promise.all([initDownloadManager(), initUploadManager()]);

    // 初始化托盘退出事件监听（必须在管理器初始化之后，确保能检测活跃任务）
    await initAppExit();
  });
</script>

<template>
  <NConfigProvider :locale="zhCN" :date-locale="dateZhCN" :theme>
    <NLoadingBarProvider>
      <NDialogProvider>
        <NMessageProvider>
          <NNotificationProvider>
            <RouterView />
          </NNotificationProvider>
        </NMessageProvider>
      </NDialogProvider>
    </NLoadingBarProvider>
  </NConfigProvider>
</template>

<style scoped></style>
