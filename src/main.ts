import { createApp } from 'vue';
import App from './App.vue';
import '@/styles/tailwind.css';

import router from './router';
import store from './store';
import { useUserStore } from './store/user';
import { useSettingStore } from './store/setting';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { delay } from 'es-toolkit';

const app = createApp(App);

app.use(store);

// 等待 store 初始化完成后再加载路由
async function bootstrap() {
  const userStore = useUserStore();
  const settingStore = useSettingStore();

  // 等待所有 store 加载完成
  await Promise.all([userStore.$tauri.start(), settingStore.$tauri.start()]);

  // 旧的记忆排序配置迁移为创建时间倒序，保留已有的自定义排序。
  if (settingStore.generalSetting.customOrder === 0) {
    settingStore.generalSetting.customOrder = 1;
    userStore.homeSortConfig = { field: 'user_ptime', direction: 'desc' };
    userStore.folderModalSortConfig = { field: 'user_ptime', direction: 'desc' };
  }

  app.use(router);

  app.mount('#app');

  // 前端渲染完成后再显示窗口，避免白屏闪烁
  await delay(400);
  await getCurrentWindow().show();
}

bootstrap();
