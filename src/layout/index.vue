<template>
  <NLayout content-class="min-h-screen" has-sider>
    <NLayoutSider
      v-model:collapsed="collapsed"
      bordered
      show-trigger="bar"
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
              <div class="w-8.5">
                <NAvatar round :src="userStore.userInfo?.user_face_l" bordered />
              </div>
              <div
                v-if="!collapsed"
                class="pl-2 line-clamp-1 font-bold"
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
        <div class="px-6 py-3">
          <NButton type="primary" @click="offlineDownloadShow = true">
            <template #icon>
              <NIcon>
                <LinkOutlined />
              </NIcon>
            </template>
            离线下载
          </NButton>
        </div>
      </NLayoutHeader>
      <NLayoutContent>
        <RouterView v-slot="{ Component }">
          <KeepAlive>
            <component :is="Component" />
          </KeepAlive>
        </RouterView>
      </NLayoutContent>
    </NLayout>
  </NLayout>
  <OfflineDownloadModal v-model:show="offlineDownloadShow" />
</template>

<script setup lang="tsx">
  import { userInfo } from '@/api/user';
  import { useUserStore } from '@/store/user';
  import { type MenuOption, NIcon } from 'naive-ui';
  import {
    CloudServerOutlined,
    DeleteOutlined,
    CloudDownloadOutlined,
    LinkOutlined,
    SettingOutlined,
  } from '@vicons/antd';
  import { RouterLink } from 'vue-router';
  import OfflineDownloadModal from './components/OfflineDownloadModal/OfflineDownloadModal.vue';
  import { getVersion } from '@/api/aria2';
  import { useSettingStore } from '@/store/setting';
  import { downloadDir } from '@tauri-apps/api/path';

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
      label: () => <RouterLink to="/setting">设置</RouterLink>,
      key: 'Setting',
      icon: () => (
        <NIcon>
          <SettingOutlined />
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
  const { open } = useWebSocket(
    `ws://localhost:${settingStore.downloadSetting.aria2Port}/jsonrpc`,
    {
      immediate: false,
      onConnected() {
        message.success('aria2服务连接成功！');
      },
    },
  );

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
      open();
    } catch (error) {
      message.error('aria2服务连接失败');
      console.error(error);
    }
  };
</script>

<style scoped></style>
