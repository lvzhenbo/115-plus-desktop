<script setup lang="ts">
  import { zhCN, dateZhCN, useOsTheme, darkTheme, lightTheme } from 'naive-ui';
  import { useSettingStore } from './store/setting';

  const settingStore = useSettingStore();
  const osThemeRef = useOsTheme();
  const theme = computed(() => {
    return osThemeRef.value === 'dark' ? darkTheme : lightTheme;
  });

  onMounted(async () => {
    const port: string = await invoke('get_port');
    settingStore.downloadSetting.aria2Port = Number(port);
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
