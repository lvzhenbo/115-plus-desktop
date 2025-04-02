<template>
  <NLayout content-class="min-h-screen" has-sider>
    <NLayoutSider
      v-model:collapsed="collapsed"
      bordered
      show-trigger="bar"
      collapse-mode="width"
      :width="140"
      :collapsed-width="66"
      :native-scrollbar="false"
    >
      <NPopover placement="right">
        <template #trigger>
          <div class="cursor-pointer transition duration-300 px-4 py-2 user-info">
            <div class="flex items-center">
              <div class="w-8.5">
                <NAvatar round :src="userStore.userInfo?.user_face_l" />
              </div>
              <div class="pl-2 line-clamp-1" v-if="!collapsed">
                {{ userStore.userInfo?.user_name }}
              </div>
            </div>
          </div>
        </template>
        <div class="large-text"> {{ userStore.userInfo?.user_name }} </div>
      </NPopover>
      <NMenu :collapsed-width="66" :collapsed-icon-size="22" :options="menuOptions" />
    </NLayoutSider>
    <NLayout>
      <NLayoutHeader bordered> 颐和园路 </NLayoutHeader>
      <NLayoutContent>Content goes here</NLayoutContent>
    </NLayout>
  </NLayout>
</template>

<script setup lang="tsx">
  import { userInfo } from '@/api/user';
  import { useUserStore } from '@/store/user';
  import { type MenuOption, NIcon } from 'naive-ui';
  import { CloudServerOutlined } from '@vicons/antd';

  const themeVars = useThemeVars();
  const userStore = useUserStore();
  const collapsed = ref(false);
  const menuOptions: MenuOption[] = [
    {
      label: '存储',
      key: 'storage',
      icon: () => (
        <NIcon>
          <CloudServerOutlined />
        </NIcon>
      ),
    },
  ];

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
