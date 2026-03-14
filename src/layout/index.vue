<template>
  <NLayout content-class="min-h-screen" has-sider>
    <NLayoutSider
      v-model:collapsed="collapsed"
      bordered
      collapse-mode="width"
      :width="180"
      :collapsed-width="66"
      :native-scrollbar="false"
    >
      <NPopover placement="right-start" class="w-96">
        <template #trigger>
          <NEl
            class="cursor-pointer transition duration-300 px-4 py-2 hover:bg-(--hover-color) rounded-(--border-radius)"
            :class="
              selectMenu === 'UserInfo'
                ? 'bg-(--primary-color)/10! dark:bg-(--primary-color)/15!'
                : ''
            "
            @click="$router.push('/userInfo')"
          >
            <div class="flex items-center">
              <div class="flex items-center w-8.5">
                <NAvatar round :src="userStore.userInfo?.user_face_l" bordered :size="30" />
              </div>
              <div
                v-if="!collapsed"
                class="pl-2 line-clamp-1 font-bold flex-1"
                :class="selectMenu === 'UserInfo' ? 'text-(--primary-color)' : ''"
              >
                {{ userStore.userInfo?.user_name }}
              </div>
            </div>
          </NEl>
        </template>
        <template #header>
          <div class="flex items-center">
            <div>
              <NAvatar round :src="userStore.userInfo?.user_face_l" size="large" bordered />
            </div>
            <div class="pl-2 flex flex-col justify-between">
              <div class="line-clamp-1">
                {{ userStore.userInfo?.user_name }}
              </div>
              <div>
                {{ userStore.userInfo?.user_id }}
              </div>
            </div>
          </div>
        </template>
        <div>
          <NProgress type="line" :percentage>
            已用 {{ userStore.userInfo?.rt_space_info.all_use.size_format }} /
            {{ userStore.userInfo?.rt_space_info.all_total.size_format }}
          </NProgress>
        </div>
        <template #footer>
          <NButton block strong quaternary type="error" @click="userStore.logout">
            退出登录
          </NButton>
        </template>
      </NPopover>
      <NMenu
        v-model:value="selectMenu"
        :collapsed-width="66"
        :collapsed-icon-size="22"
        :options="menuOptions"
      />
    </NLayoutSider>
    <NLayout>
      <NLayoutHeader bordered>
        <div class="px-4 py-3 flex items-center justify-between">
          <NSpace>
            <NButton quaternary circle @click="collapsed = !collapsed">
              <template #icon>
                <NIcon>
                  <component :is="collapsed ? MenuUnfoldOutlined : MenuFoldOutlined" />
                </NIcon>
              </template>
            </NButton>
            <NButton type="primary" @click="offlineDownloadShow = true">
              <template #icon>
                <NIcon>
                  <LinkOutlined />
                </NIcon>
              </template>
              离线下载
            </NButton>
          </NSpace>
          <NButton round secondary @click="searchShow = true">
            <template #icon>
              <NIcon>
                <SearchOutlined />
              </NIcon>
            </template>
            搜索
          </NButton>
        </div>
      </NLayoutHeader>
      <NLayoutContent :native-scrollbar="false" class="h-[calc(100vh-59px)]">
        <RouterView v-slot="{ Component }">
          <Transition
            mode="out-in"
            enter-active-class="transition-opacity duration-200"
            leave-active-class="transition-opacity duration-200"
            enter-from-class="opacity-0"
            leave-from-class="opacity-100"
            enter-to-class="opacity-100"
            leave-to-class="opacity-0"
          >
            <KeepAlive>
              <component :is="Component" :key="route.name" />
            </KeepAlive>
          </Transition>
        </RouterView>
      </NLayoutContent>
    </NLayout>
  </NLayout>
  <OfflineDownloadModal v-model:show="offlineDownloadShow" />
  <SearchModal v-model:show="searchShow" />
</template>

<script setup lang="tsx">
  import { userInfo } from '@/api/user';
  import { useUserStore } from '@/store/user';
  import type { MenuOption } from 'naive-ui';
  import {
    CloudServerOutlined,
    DeleteOutlined,
    CloudDownloadOutlined,
    LinkOutlined,
    SettingOutlined,
    DownloadOutlined,
    UploadOutlined,
    SearchOutlined,
    InfoCircleOutlined,
    MenuFoldOutlined,
    MenuUnfoldOutlined,
  } from '@vicons/antd';
  import OfflineDownloadModal from './components/OfflineDownloadModal/OfflineDownloadModal.vue';
  import SearchModal from './components/SearchModal/SearchModal.vue';
  import { getVersion } from '@/api/aria2';
  import { useSettingStore } from '@/store/setting';
  import { downloadDir } from '@tauri-apps/api/path';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { ask } from '@tauri-apps/plugin-dialog';
  import { useDownloadManager } from '@/composables/useDownloadManager';
  import { useUploadManager } from '@/composables/useUploadManager';
  import { useCheckUpdate } from '@/composables/useCheckUpdate';
  import { hasActiveDownloads } from '@/db/downloads';
  import { getActiveUploads } from '@/db/uploads';

  const route = useRoute();
  const userStore = useUserStore();
  const collapsed = ref(false);
  const menuOptions: MenuOption[] = [
    {
      label: () => <RouterLink to="/home">我的文件</RouterLink>,
      key: 'Home',
      icon: () => (
        <NIcon>
          <CloudServerOutlined />
        </NIcon>
      ),
    },
    {
      label: () => <RouterLink to="/recycleBin">回收站</RouterLink>,
      key: 'RecycleBin',
      icon: () => (
        <NIcon>
          <DeleteOutlined />
        </NIcon>
      ),
    },
    {
      label: () => <RouterLink to="/cloudDownload">云下载</RouterLink>,
      key: 'CloudDownload',
      icon: () => (
        <NIcon>
          <CloudDownloadOutlined />
        </NIcon>
      ),
    },
    {
      label: () => <RouterLink to="/download">下载列表</RouterLink>,
      key: 'Download',
      icon: () => (
        <NIcon>
          <DownloadOutlined />
        </NIcon>
      ),
    },
    {
      label: () => <RouterLink to="/upload">上传列表</RouterLink>,
      key: 'Upload',
      icon: () => (
        <NIcon>
          <UploadOutlined />
        </NIcon>
      ),
    },
    {
      label: () => <RouterLink to="/setting">设置</RouterLink>,
      key: 'Setting',
      icon: () => (
        <NIcon>
          <SettingOutlined />
        </NIcon>
      ),
    },
    {
      label: () => <RouterLink to="/about">关于</RouterLink>,
      key: 'About',
      icon: () => (
        <NIcon>
          <InfoCircleOutlined />
        </NIcon>
      ),
    },
  ];
  const selectMenu = ref<string>(route.name as string);
  const percentage = computed(() => {
    if (userStore.userInfo) {
      return Math.round(
        (userStore.userInfo.rt_space_info.all_use.size /
          userStore.userInfo.rt_space_info.all_total.size) *
          100,
      );
    } else {
      return 0;
    }
  });
  const message = useMessage();
  const settingStore = useSettingStore();
  const offlineDownloadShow = ref(false);
  const searchShow = ref(false);
  const { pauseAllTasks: pauseAllDownloads } = useDownloadManager();
  const { pauseAllTasks: pauseAllUploads } = useUploadManager();
  const { checkForUpdate } = useCheckUpdate();

  /** 暂停所有任务并关闭窗口 */
  const pauseAndClose = async () => {
    await Promise.all([pauseAllDownloads(), pauseAllUploads()]);
    await getCurrentWindow().destroy();
  };

  /** 检查是否有活跃任务 */
  const hasActiveTasks = async () => {
    const [downloads, uploads] = await Promise.all([hasActiveDownloads(), getActiveUploads()]);
    return downloads || uploads.length > 0;
  };

  watch(
    () => route.name,
    (newVal) => {
      selectMenu.value = newVal as string;
    },
  );

  onMounted(async () => {
    const port: number = await invoke('get_port');
    settingStore.downloadSetting.aria2Port = port;
    getUserInfo();
    getAria2Version();
    if (!settingStore.downloadSetting.downloadPath) {
      settingStore.downloadSetting.downloadPath = await downloadDir();
    }

    // 启动时自动检查更新
    if (settingStore.generalSetting.autoCheckUpdate) {
      checkForUpdate({ silent: true });
    }

    // 监听窗口关闭事件
    await getCurrentWindow().onCloseRequested(async (event) => {
      const active = await hasActiveTasks();
      if (!active) return; // 无活跃任务直接关闭

      if (settingStore.generalSetting.skipExitConfirm) {
        // 自动暂停并关闭
        event.preventDefault();
        await pauseAndClose();
        return;
      }

      // 弹出二次确认
      event.preventDefault();
      const confirmed = await ask('当前有正在进行的传输任务，确定退出？', {
        title: '提示',
        kind: 'warning',
        okLabel: '确定',
        cancelLabel: '取消',
      });
      if (confirmed) {
        await pauseAndClose();
      }
    });
  });

  const getUserInfo = async () => {
    try {
      const res = await userInfo();
      userStore.userInfo = res.data;
    } catch (_error) {}
  };

  const getAria2Version = async () => {
    try {
      await getVersion();
      message.success('aria2服务连接成功！');
    } catch (error) {
      message.error('aria2服务连接失败');
      console.error(error);
    }
  };
</script>

<style scoped></style>
