import { createPlugin } from '@tauri-store/pinia';

const store = createPinia();
store.use(createPlugin());

export default store;
