<template>
  <NModal v-model:show="show" preset="card" class="w-200!" title="添加云下载">
    <NInput
      v-model:value="data.urls"
      type="textarea"
      placeholder="支持多个链接url,换行符分隔，支持HTTP(S)、FTP、磁力链和电驴链接"
      clearable
      :disabled="loading || preparing"
      :rows="10"
    />
    <NAlert v-if="torrentRef?.status.errorMessage" type="error" class="mt-2">
      {{ torrentRef.status.errorMessage }}
      <NButton
        v-if="torrentRef.status.canRetry"
        class="ml-2"
        :disabled="loading || preparing"
        @click="torrentRef?.prepare()"
      >
        重试
      </NButton>
    </NAlert>
    <NInputGroup class="mt-2">
      <NInputGroupLabel>保存到：</NInputGroupLabel>
      <NInput :value="folderName" placeholder="请选择文件夹" readonly />
      <NButton type="primary" :disabled="loading || preparing" @click="folderModalShow = true">
        选择文件夹
      </NButton>
    </NInputGroup>
    <template #action>
      <div class="flex items-center justify-between gap-3">
        <div class="flex items-center gap-3">
          <NButton :loading="preparing" :disabled="loading || preparing" @click="handleAddTorrent">
            添加BT任务
          </NButton>
          <div>
            本月配额：剩 {{ countData.count - countData.used }} / 总 {{ countData.count }}
          </div>
        </div>
        <NButton
          type="primary"
          :disabled="countData.used >= countData.count || preparing"
          :loading
          @click="handleDownload('url')"
        >
          开始下载
        </NButton>
      </div>
    </template>
  </NModal>
  <TorrentDownload
    v-if="show || torrentShow"
    ref="torrentRef"
    v-model:show="torrentShow"
    v-model:task="torrentTask"
    v-model:busy="preparing"
    :target-cid="torrentCid"
    :disabled="loading"
    @ready="handleTorrentReady"
  >
    <NInputGroup class="mt-2">
      <NInputGroupLabel>保存到：</NInputGroupLabel>
      <NInput :value="folderName" placeholder="请选择文件夹" readonly />
      <NButton type="primary" :disabled="loading" @click="folderModalShow = true">
        选择文件夹
      </NButton>
    </NInputGroup>
    <template #action>
      <div class="flex items-center justify-between gap-3">
        <div> 本月配额：剩 {{ countData.count - countData.used }} / 总 {{ countData.count }} </div>
        <NButton
          type="primary"
          :disabled="countData.used >= countData.count || !torrentTask"
          :loading
          @click="handleDownload('bt')"
        >
          开始下载
        </NButton>
      </div>
    </template>
  </TorrentDownload>
  <FolderModal v-model:show="folderModalShow" @select="handleFolderSelect"></FolderModal>
</template>

<script setup lang="ts">
  import { quotaInfo, urlTaskAdd, torrentTaskAdd } from '@/api/cloud';
  import { fileDetail } from '@/api/file';
  import type { QuotaInfoResponseData, TorrentTaskParams } from '@/api/types/cloud';
  import { useUserStore } from '@/store/user';
  import TorrentDownload from './TorrentDownload.vue';

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
  const folderName = ref('');
  const userStore = useUserStore();
  const loading = ref(false);
  const torrentRef = useTemplateRef('torrentRef');
  const torrentShow = ref(false);
  const torrentCid = ref('0');
  const visible = computed(() => show.value || torrentShow.value);
  const activeCid = computed(() => (torrentShow.value ? torrentCid.value : data.value.wp_path_id));
  const torrentTask = shallowRef<TorrentTaskParams | null>(null);
  const preparing = ref(false);
  let dialogVersion = 0;

  watch([show, torrentShow], ([urlOpen, btOpen], [wasUrlOpen, wasBtOpen]) => {
    if ((urlOpen || btOpen) !== (wasUrlOpen || wasBtOpen)) dialogVersion++;
    if (urlOpen && !wasUrlOpen) {
      getQuotaInfo();
      data.value.wp_path_id = userStore.getLatestFolder('save') || '0';
    }
    if (!urlOpen) {
      data.value.urls = '';
      data.value.wp_path_id = '0';
      folderModalShow.value = false;
    }
    if (!urlOpen && !btOpen) loading.value = false;
  });

  watch([visible, activeCid], async ([visible, cid], _previous, onCleanup) => {
    folderName.value = '';
    if (!visible) return;
    if (cid === '0') {
      folderName.value = '全部文件';
      return;
    }

    let active = true;
    onCleanup(() => {
      active = false;
    });
    try {
      const response = await fileDetail({ file_id: cid });
      if (active) folderName.value = response.data.file_name;
    } catch (error) {
      if (active) message.error('获取文件夹名称失败，请重新选择文件夹');
      console.error(error);
    }
  });

  const getQuotaInfo = async () => {
    const version = dialogVersion;
    const response = await quotaInfo();
    if (visible.value && version === dialogVersion) countData.value = response.data;
  };

  const handleAddTorrent = () => {
    torrentCid.value = data.value.wp_path_id;
    torrentRef.value?.handleChoose();
  };

  const handleTorrentReady = () => {
    if (!show.value) return;
    torrentShow.value = true;
    show.value = false;
  };

  const handleDownload = async (mode: 'url' | 'bt') => {
    if (loading.value || preparing.value) return;
    if (mode === 'url' && !data.value.urls.trim()) {
      message.error('请输入下载链接');
      return;
    }
    if (mode === 'bt' && !torrentTask.value) {
      message.error('请选择要下载的文件');
      return;
    }
    const version = dialogVersion;
    const cid = mode === 'bt' ? torrentCid.value : data.value.wp_path_id;
    try {
      loading.value = true;
      if (mode === 'bt') {
        await torrentTaskAdd({ ...torrentTask.value!, wp_path_id: cid });
      } else {
        await urlTaskAdd({ ...data.value });
      }
      if (!(mode === 'bt' ? torrentShow.value : show.value) || version !== dialogVersion) return;
      message.success('添加云下载成功');
      userStore.setLatestFolder('save', cid);
      if (mode === 'bt') torrentShow.value = false;
      else show.value = false;
    } catch (error) {
      console.error(error);
    } finally {
      if (version === dialogVersion) loading.value = false;
    }
  };

  const handleFolderSelect = (cid: string) => {
    if (torrentShow.value) torrentCid.value = cid;
    else data.value.wp_path_id = cid;
  };
</script>

<style scoped></style>
