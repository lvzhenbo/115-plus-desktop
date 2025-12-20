import { createApp } from 'vue';
import App from './App.vue';
import '@/styles/tailwind.css';

import router from './router';
import store from './store';
import { useUserStore } from './store/user';
import { useSettingStore } from './store/setting';

const app = createApp(App);

app.use(store);

// 等待 store 初始化完成后再加载路由
async function bootstrap() {
  const userStore = useUserStore();
  const settingStore = useSettingStore();

  // 等待所有 store 加载完成
  await Promise.all([userStore.$tauri.start(), settingStore.$tauri.start()]);

  app.use(router);

  app.mount('#app');
}

bootstrap();
