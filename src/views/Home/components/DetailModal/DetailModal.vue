<template>
  <NModal v-model:show="show" preset="card" class="w-150!" :title="fileDetailData?.file_name">
    <NDescriptions label-placement="left" :column="1">
      <NDescriptionsItem label="类型">
        {{ fileDetailData?.file_category === '0' ? '文件夹' : '文件' }}
      </NDescriptionsItem>
      <NDescriptionsItem label="大小"> {{ fileDetailData?.size }} </NDescriptionsItem>
      <NDescriptionsItem label="SHA1" v-if="fileDetailData?.file_category === '1'">
        {{ fileDetailData?.sha1 }}
        <NButton
          type="primary"
          secondary
          size="tiny"
          @click="copy(fileDetailData?.sha1)"
          v-if="isSupported"
        >
          <template #icon>
            <NIcon>
              <CopyOutlined />
            </NIcon>
          </template>
          {{ copied ? '已复制' : '复制' }}
        </NButton>
      </NDescriptionsItem>
      <NDescriptionsItem label="包含" v-if="fileDetailData?.file_category === '0'">
        {{ fileDetailData?.count }} 个文件， {{ fileDetailData?.folder_count }} 个文件夹
      </NDescriptionsItem>
      <NDescriptionsItem label="音视频时长" v-if="fileDetailData?.play_long">
        {{ formatSeconds(fileDetailData.play_long) }}
      </NDescriptionsItem>
      <NDescriptionsItem label="创建时间">
        {{ fileDetailData?.ptime ? formatDate(fileDetailData?.ptime) : '' }}
      </NDescriptionsItem>
      <NDescriptionsItem label="修改时间">
        {{ fileDetailData?.utime ? formatDate(fileDetailData?.utime) : '' }}
      </NDescriptionsItem>
      <NDescriptionsItem label="上次打开时间">
        {{ fileDetailData?.open_time ? formatDate(fileDetailData?.open_time) : '' }}
      </NDescriptionsItem>
      <NDescriptionsItem label="位置">
        <span v-for="(item, index) in fileDetailData?.paths" :key="item.file_id">
          {{ fileDetailData?.paths ? item.file_name : '' }}
          <span
            v-if="fileDetailData?.paths && index < fileDetailData.paths.length - 1"
            class="mx-2"
          >
            /
          </span>
        </span>
      </NDescriptionsItem>
    </NDescriptions>
  </NModal>
</template>

<script setup lang="ts">
  import type { FileDeatil } from '@/api/types/file';
  import { format } from 'date-fns';
  import { intervalToDuration } from 'date-fns';
  import { CopyOutlined } from '@vicons/antd';

  const show = defineModel('show', {
    type: Boolean,
    default: false,
  });

  defineProps<{
    fileDetailData: FileDeatil | null;
  }>();

  const { copy, copied, isSupported } = useClipboard();

  const formatDate = (date: string | number) => {
    return format(new Date(Number(date) * 1000), 'yyyy-MM-dd HH:mm:ss');
  };

  const formatSeconds = (seconds: number) => {
    const duration = intervalToDuration({ start: 0, end: seconds * 1000 });
    let result = '';

    if (duration.hours) {
      result += `${duration.hours}小时 `;
    }
    if (duration.minutes) {
      result += `${duration.minutes}分钟 `;
    }
    if (duration.seconds || (!duration.hours && !duration.minutes)) {
      result += `${duration.seconds}秒`;
    }

    return result;
  };
</script>

<style scoped></style>
