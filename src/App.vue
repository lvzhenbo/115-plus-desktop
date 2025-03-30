<script setup lang="ts">
  // const greetMsg = ref('');
  // const name = ref('');

  // async function greet() {
  //   // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
  //   greetMsg.value = await invoke('greet', { name: name.value });
  // }
  import { zhCN, dateZhCN, useOsTheme, darkTheme, lightTheme } from 'naive-ui';

  const osThemeRef = useOsTheme();
  const theme = computed(() => {
    return osThemeRef.value === 'dark' ? darkTheme : lightTheme;
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
