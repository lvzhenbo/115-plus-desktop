<template>
  <NModal v-model:show="show" preset="card" class="w-200!" title="添加离线下载">
    <NInput
      v-model:value="data.urls"
      type="textarea"
      placeholder="支持多个链接url,换行符分隔，支持HTTP(S)、FTP、磁力链和电驴链接"
      clearable
      :rows="10"
    />
    <template #action>
      <div class="flex justify-between">
        <div> 本月配额：剩{{ countData.count - countData.used }} / 总{{ countData.count }} </div>
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
</template>

<script setup lang="ts">
  import { quotaInfo, urlTaskAdd } from '@/api/cloud';
  import type { QuotaInfoResponseData } from '@/api/types/cloud';
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

  watch(show, (val) => {
    if (val) {
      getQuotaInfo();
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
      show.value = false;
    } catch (error) {
      console.error(error);
    }
  };
</script>

<style scoped></style>
