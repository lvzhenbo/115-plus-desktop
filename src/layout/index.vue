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
          <div class="cursor-pointer transition duration-300 px-4 py-2 user-info">
            <div class="flex items-center">
              <div class="w-8.5">
                <NAvatar round :src="userStore.userInfo?.user_face_l" bordered />
              </div>
              <div v-if="!collapsed" class="pl-2 line-clamp-1 font-bold">
                {{ userStore.userInfo?.user_name }}
              </div>
            </div>
          </div>
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
      <NLayoutHeader bordered> 颐和园路 </NLayoutHeader>
      <NLayoutContent>
        <RouterView />
      </NLayoutContent>
    </NLayout>
  </NLayout>
</template>

<script setup lang="tsx">
  import { userInfo } from '@/api/user';
  import { useUserStore } from '@/store/user';
  import { type MenuOption, NIcon } from 'naive-ui';
  import { CloudServerOutlined } from '@vicons/antd';
  import { RouterLink } from 'vue-router';

  const route = useRoute();
  const themeVars = useThemeVars();
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

  onMounted(() => {
    getUserInfo();
  });

  const getUserInfo = async () => {
    try {
      const res = await userInfo();
      userStore.userInfo = res.data;
    } catch (_error) {}
  };
</script>

<style scoped>
  .user-info:hover {
    background-color: v-bind('themeVars.hoverColor');
    border-radius: v-bind('themeVars.borderRadius');
  }
</style>
