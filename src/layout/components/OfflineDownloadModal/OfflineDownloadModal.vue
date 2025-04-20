<template>
  <NModal v-model:show="show" preset="card" class="w-200!" title="添加离线下载">
    <NInput
      v-model:value="data.urls"
      type="textarea"
      placeholder="支持多个链接url,换行符分隔，支持HTTP(S)、FTP、磁力链和电驴链接"
      clearable
      :rows="10"
    />
    <NInputGroup class="mt-2">
      <NInputGroupLabel>保存到文件夹的ID：</NInputGroupLabel>
      <NInput v-model:value="data.wp_path_id" placeholder="请输入文件夹ID" readonly />
      <NButton type="primary" @click="folderModalShow = true"> 选择文件夹 </NButton>
    </NInputGroup>
    <template #action>
      <div class="flex justify-between">
        <div> 本月配额：剩 {{ countData.count - countData.used }} / 总 {{ countData.count }} </div>
        <div>
          <NButton
            type="primary"
            :disabled="countData.used >= countData.count"
            @click="handleDownload"
          >
            开始下载
          </NButton>
        </div>
      </div>
    </template>
  </NModal>
  <FolderModal v-model:show="folderModalShow" @select="handleFolderSelect"></FolderModal>
</template>

<script setup lang="ts">
  import { quotaInfo, urlTaskAdd } from '@/api/cloud';
  import type { QuotaInfoResponseData } from '@/api/types/cloud';
  import { useUserStore } from '@/store/user';
  import { trim } from 'radash';

  const show = defineModel('show', {
    type: Boolean,
    default: false,
  });

  const message = useMessage();
  const data = ref({
    urls: '',
    wp_path_id: '0',
  });
  const countData = ref<QuotaInfoResponseData>({
    count: 0,
    used: 0,
  });
  const folderModalShow = ref(false);
  const userStore = useUserStore();

  watch(show, (val) => {
    if (val) {
      getQuotaInfo();
      data.value.wp_path_id = userStore.getLatestFolder('save') || '0';
    } else {
      data.value.urls = '';
      data.value.wp_path_id = '0';
    }
  });

  const getQuotaInfo = async () => {
    const response = await quotaInfo();
    countData.value = response.data;
  };

  const handleDownload = async () => {
    if (!trim(data.value.urls, '\n')) {
      message.error('请输入下载链接');
      return;
    }
    try {
      await urlTaskAdd(data.value);
      message.success('添加离线下载成功');
      userStore.setLatestFolder('save', data.value.wp_path_id);
      show.value = false;
    } catch (error) {
      console.error(error);
    }
  };

  const handleFolderSelect = (cid: string) => {
    data.value.wp_path_id = cid;
  };
</script>

<style scoped></style>
