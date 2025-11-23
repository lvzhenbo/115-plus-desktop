<script setup lang="ts">
  import { zhCN, dateZhCN, darkTheme, lightTheme } from 'naive-ui';
  import { listen } from '@tauri-apps/api/event';
  import { useUserStore } from '@/store/user';

  const osThemeRef = useOsTheme();
  const theme = computed(() => {
    return osThemeRef.value === 'dark' ? darkTheme : lightTheme;
  });

  const userStore = useUserStore();

  // 监听其他窗口的token更新事件
  onMounted(() => {
    listen('token-updated', () => {
      // 使用$hydrate重新从持久化存储加载数据
      userStore.$hydrate();
    });
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
