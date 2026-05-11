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
      <NPopover placement="right-start" class="w-96" :delay="500">
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
          <NButton block strong quaternary type="error" @click="handleLogout"> 退出登录 </NButton>
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
  import { useSettingStore } from '@/store/setting';
  import { downloadDir } from '@tauri-apps/api/path';
  import { useDownloadManager } from '@/composables/useDownloadManager';
  import { useUploadManager } from '@/composables/useUploadManager';
  import { useCheckUpdate } from '@/composables/useCheckUpdate';

  const route = useRoute();
  const userStore = useUserStore();
  const collapsed = ref(false);
  // 侧边栏入口和路由一一对应，保持应用级导航收口在布局层。
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
  const settingStore = useSettingStore();
  const offlineDownloadShow = ref(false);
  const searchShow = ref(false);

  // 退出登录前暂停所有传输任务，避免 token 失效后任务异常。
  const { pauseAllTasks: pauseAllDownloads } = useDownloadManager();
  const { pauseAllTasks: pauseAllUploads } = useUploadManager();
  const { checkForUpdate } = useCheckUpdate();

  const handleLogout = async () => {
    await Promise.all([pauseAllDownloads(), pauseAllUploads()]);
    userStore.logout();
  };

  // 保持菜单高亮与当前路由一致。
  watch(
    () => route.name,
    (newVal) => {
      selectMenu.value = newVal as string;
    },
  );

  onMounted(async () => {
    getUserInfo();
    if (!settingStore.downloadSetting.downloadPath) {
      settingStore.downloadSetting.downloadPath = await downloadDir();
    }

    // 登录后自动检查更新
    if (settingStore.generalSetting.autoCheckUpdate) {
      void checkForUpdate({ silent: true });
    }
  });

  // 布局层统一拉取一次用户资料，避免各个页面重复请求。
  const getUserInfo = async () => {
    try {
      const res = await userInfo();
      userStore.userInfo = res.data;
    } catch (_error) {}
  };
</script>

<style scoped></style>
