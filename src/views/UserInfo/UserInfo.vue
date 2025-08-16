<template>
  <div class="px-6 py-3">
    <NDescriptions label-placement="left" title="个人信息" :column="1">
      <NDescriptionsItem label="头像">
        <!-- @vue-expect-error -->
        <NImage
          width="180"
          :src="userStore.userInfo?.user_face_l"
          :render-toolbar="renderToolbar"
          show-toolbar-tooltip
        />
      </NDescriptionsItem>
      <NDescriptionsItem label="用户名"> {{ userStore.userInfo?.user_name }} </NDescriptionsItem>
      <NDescriptionsItem label="用户ID"> {{ userStore.userInfo?.user_id }} </NDescriptionsItem>
      <NDescriptionsItem label="会员信息">
        {{ userStore.userInfo?.vip_info.level_name }}
        {{
          userStore.userInfo?.vip_info.expire
            ? `${format(
                new Date(userStore.userInfo.vip_info.expire * 1000),
                'yyyy-MM-dd HH:mm:ss （eeee）',
                {
                  locale: zhCN,
                },
              )} 到期`
            : ''
        }}
      </NDescriptionsItem>
      <NDescriptionsItem label="总空间">
        {{ userStore.userInfo?.rt_space_info.all_total.size_format }}
      </NDescriptionsItem>
      <NDescriptionsItem label="已用空间">
        {{ userStore.userInfo?.rt_space_info.all_use.size_format }}
      </NDescriptionsItem>
      <NDescriptionsItem label="剩余空间">
        {{ userStore.userInfo?.rt_space_info.all_remain.size_format }}
      </NDescriptionsItem>
    </NDescriptions>
  </div>
</template>

<script setup lang="ts">
  import { userInfo } from '@/api/user';
  import { useUserStore } from '@/store/user';
  import { format } from 'date-fns';
  import { zhCN } from 'date-fns/locale/zh-CN';
  import type { ImageRenderToolbarProps } from 'naive-ui';

  const userStore = useUserStore();

  onMounted(() => {
    getUserInfo();
  });

  onActivated(() => {
    getUserInfo();
  });

  const getUserInfo = async () => {
    try {
      const res = await userInfo();
      userStore.userInfo = res.data;
    } catch (_error) {}
  };

  const renderToolbar = ({ nodes }: ImageRenderToolbarProps) => {
    return [
      nodes.rotateCounterclockwise,
      nodes.rotateClockwise,
      nodes.resizeToOriginalSize,
      nodes.zoomOut,
      nodes.zoomIn,
      nodes.close,
    ];
  };
</script>

<style scoped></style>
