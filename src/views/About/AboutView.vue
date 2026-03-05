<template>
  <div class="p-4 flex flex-col items-center">
    <div class="flex flex-col items-center mb-6">
      <img src="/icon.svg" alt="115+" class="w-20 h-20 mb-4" />
      <NText class="mb-1 text-3xl">115+</NText>
      <NText depth="3">基于 115 网盘开放平台的第三方开源桌面客户端</NText>
    </div>

    <NDescriptions label-placement="left" bordered :column="1" class="max-w-lg w-full">
      <NDescriptionsItem label="应用版本">
        <NTag type="primary" size="small" round>v{{ appVersion }}</NTag>
      </NDescriptionsItem>
      <NDescriptionsItem label="Tauri 版本">
        <NTag type="info" size="small" round>v{{ tauriVersion }}</NTag>
      </NDescriptionsItem>
      <NDescriptionsItem label="作者">
        <NAvatar
          src="https://avatars.githubusercontent.com/u/32427677?v=4"
          :fallback-src="LvzhenboAvatar"
          class="cursor-pointer"
          name="lvzhenbo"
          @click="openUrl('https://github.com/lvzhenbo')"
        />
      </NDescriptionsItem>
      <NDescriptionsItem label="开源协议">MIT License</NDescriptionsItem>
      <NDescriptionsItem label="项目地址">
        <NButton text @click="openUrl('https://github.com/lvzhenbo/115-plus-desktop')">
          <template #icon>
            <NIcon>
              <GithubOutlined />
            </NIcon>
          </template>
          Github
        </NButton>
      </NDescriptionsItem>
    </NDescriptions>

    <div class="mt-6 flex gap-3">
      <NButton type="primary" :loading="isChecking" @click="checkForUpdate({ silent: false })">
        <template #icon>
          <NIcon><UpdateOutlined /></NIcon>
        </template>
        检查更新
      </NButton>
      <NButton @click="openUrl('https://github.com/lvzhenbo/115-plus-desktop/issues')">
        <template #icon>
          <NIcon><ExclamationCircleOutlined /></NIcon>
        </template>
        问题反馈
      </NButton>
    </div>

    <NDivider />

    <div class="max-w-lg w-full">
      <NH3>技术栈</NH3>
      <NSpace>
        <NTag v-for="tech in techStack" :key="tech" round>{{ tech }}</NTag>
      </NSpace>
    </div>

    <div class="mt-4 max-w-lg w-full">
      <NH3>功能特性</NH3>
      <NUl>
        <NLi>手机扫码登录</NLi>
        <NLi>文件管理（复制、移动、删除、重命名等）</NLi>
        <NLi>文件上传与下载</NLi>
        <NLi>文件搜索</NLi>
        <NLi>回收站管理</NLi>
        <NLi>视频在线播放（支持字幕、进度记忆）</NLi>
        <NLi>云下载（链接下载、BT 种子解析下载）</NLi>
      </NUl>
    </div>

    <NText depth="3" class="mt-6"> © 2025 lvzhenbo. Released under the MIT License. </NText>
  </div>
</template>

<script setup lang="ts">
  import { getVersion, getTauriVersion } from '@tauri-apps/api/app';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { ExclamationCircleOutlined, GithubOutlined } from '@vicons/antd';
  import { UpdateOutlined } from '@vicons/material';
  import LvzhenboAvatar from '@/assets/lvzhenbo.gif';
  import { useCheckUpdate } from '@/composables/useCheckUpdate';

  const { isChecking, checkForUpdate } = useCheckUpdate();

  const appVersion = ref('');
  const tauriVersion = ref('');

  const techStack = ['Tauri 2', 'Vue 3', 'Naive UI', 'Tailwind CSS', 'Alova', 'Aria2', 'Rust'];

  onMounted(async () => {
    appVersion.value = await getVersion();
    tauriVersion.value = await getTauriVersion();
  });
</script>

<style scoped></style>
