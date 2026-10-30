<template>
  <NModal v-model:show="show" preset="card" class="w-250!" title="选择下载文件">
    <NEl v-if="torrent" class="mt-2 text-(--text-color-2)">
      <div class="mb-2 truncate" :title="torrent.torrent_name">
        <NText strong>{{ torrent.torrent_name }}</NText>
        <NText depth="3" class="ml-2">{{
          filesize(torrent.file_size, { standard: 'jedec' })
        }}</NText>
      </div>
      <ListHeader
        :items
        :selected-items="selectedItems"
        :columns
        show-checkbox
        sort-disabled
        @toggle-select-all="toggleSelectAll"
      />
      <NScrollbar class="h-64">
        <FileItemView
          v-for="item in pageItems"
          :key="item.fid"
          :item
          :selected="selectedItems.has(item.fid)"
          :columns
          show-checkbox
          view-mode="list"
          @click="toggleItem(item.fid)"
          @check="toggleItem(item.fid)"
        />
      </NScrollbar>
      <ExplorerStatusbar
        :total-count="items.length"
        :selected-count="selectedItems.size"
        :current-page="page"
        :total-pages="Math.ceil(items.length / pageSize)"
        :page-size="pageSize"
        @update:current-page="page = $event"
        @update:page-size="handlePageSize"
      />
    </NEl>
    <slot />
    <template #action><slot name="action" /></template>
  </NModal>
</template>

<script setup lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { invoke } from '@tauri-apps/api/core';
  import { filesize } from 'filesize';
  import { torrentParse } from '@/api/cloud';
  import type { TorrentInfo, TorrentTaskParams } from '@/api/types/cloud';
  import { useUploadManager, type UploadFile } from '@/composables/useUploadManager';
  import type { ListColumn } from '@/components/FileExplorer/types';
  import FileItemView from '@/components/FileExplorer/components/FileItemView/FileItemView.vue';
  import ListHeader from '@/components/FileExplorer/components/ListHeader/ListHeader.vue';
  import ExplorerStatusbar from '@/components/FileExplorer/components/ExplorerStatusbar/ExplorerStatusbar.vue';

  const show = defineModel<boolean>('show', { required: true });
  const emit = defineEmits<{ ready: [] }>();
  const props = defineProps<{ targetCid: string; disabled: boolean }>();
  const task = defineModel<TorrentTaskParams | null>('task', { required: true });
  const busy = defineModel<boolean>('busy', { required: true });
  const { displayList, uploadFile, retryTask, resumeTask } = useUploadManager();
  const filePath = ref('');
  const fileName = ref('');
  const uploadId = ref<string>();
  const seed = shallowRef<{ torrent_sha1: string; pick_code: string }>();
  const torrent = shallowRef<TorrentInfo>();
  const errorMessage = ref('');
  const selectedItems = ref(new Set<string>());
  const columns: ListColumn[] = ['size'];
  const page = ref(1);
  const pageSize = ref(50);
  let controller: AbortController | undefined;
  let disposed = false;

  const currentUpload = computed(() =>
    displayList.value.find((item) => item.id === uploadId.value),
  );
  // 保留解析结果的原始顺序和索引，分页和勾选不改变提交给 115 的 wanted。
  const items = computed(() =>
    (torrent.value?.torrent_filelist || []).map((file, index) => ({
      fid: String(index),
      fn: file.path,
      fc: '1',
      ico: file.path.split('.').pop()?.toLowerCase() || '',
      fs: file.size,
    })),
  );
  const pageItems = computed(() =>
    items.value.slice((page.value - 1) * pageSize.value, page.value * pageSize.value),
  );

  watchEffect(() => {
    const wanted = items.value.filter((item) => selectedItems.value.has(item.fid));
    task.value =
      torrent.value && seed.value && wanted.length
        ? {
            ...seed.value,
            info_hash: torrent.value.info_hash,
            wanted: wanted.map((item) => item.fid).join(','),
            save_path: torrent.value.torrent_name,
          }
        : null;
  });

  function toggleItem(id: string) {
    if (props.disabled) return;
    const next = new Set(selectedItems.value);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selectedItems.value = next;
  }

  function toggleSelectAll() {
    if (props.disabled) return;
    selectedItems.value =
      selectedItems.value.size === items.value.length
        ? new Set()
        : new Set(items.value.map((item) => item.fid));
  }

  function handlePageSize(size: number) {
    pageSize.value = size;
    page.value = 1;
  }

  function waitForUpload(
    id: string,
    signal: AbortSignal,
    previousTask?: UploadFile,
  ): Promise<UploadFile> {
    return new Promise((resolve, reject) => {
      let seen = false;
      const cleanup = () => {
        stop();
        signal.removeEventListener('abort', abort);
      };
      const fail = (error: Error) => {
        cleanup();
        reject(error);
      };
      const abort = () => fail(new DOMException('已取消种子解析', 'AbortError'));
      const check = () => {
        const file = displayList.value.find((item) => item.id === id);
        if (!file) {
          if (seen) fail(new Error('上传任务已移除，请重新选择种子文件'));
          return;
        }
        seen = true;
        // 重试/继续命令返回时，状态同步可能尚未替换旧快照。
        if (file === previousTask) return;
        if (file.status === 'complete') {
          cleanup();
          resolve(file);
        } else if (file.status === 'error' || file.status === 'cancelled') {
          fail(new Error(file.errorMessage || '种子上传失败，请重试'));
        } else if (file.status === 'paused') {
          fail(new Error('种子上传已暂停，请重试'));
        }
      };
      const stop = watch(displayList, check, { flush: 'sync' });
      signal.addEventListener('abort', abort, { once: true });
      if (signal.aborted) abort();
      else check();
    });
  }

  async function prepare() {
    if (busy.value || props.disabled || !filePath.value) return;
    const run = new AbortController();
    controller = run;
    const { signal } = run;
    busy.value = true;
    errorMessage.value = '';
    torrent.value = undefined;
    selectedItems.value = new Set();
    const targetCid = props.targetCid;
    try {
      if (!seed.value) {
        const existing = currentUpload.value;
        let previousTask: UploadFile | undefined;
        if (existing?.status === 'error' || existing?.status === 'cancelled') {
          previousTask = existing;
          await retryTask(existing);
        } else if (existing?.status === 'paused') {
          previousTask = existing;
          await resumeTask(existing);
        }
        if (!existing) {
          const size = await invoke<number>('upload_get_file_size', { filePath: filePath.value });
          if (signal.aborted) return;
          uploadId.value = await uploadFile(filePath.value, fileName.value, size, targetCid);
          if (!uploadId.value) throw new Error('种子上传任务创建失败，请重试');
        }
        if (signal.aborted) return;
        const uploaded = await waitForUpload(uploadId.value!, signal, previousTask);
        if (signal.aborted) return;
        if (!uploaded.sha1 || !uploaded.pickCode) {
          throw new Error('种子上传信息不完整，请重新选择种子文件');
        }
        seed.value = { torrent_sha1: uploaded.sha1, pick_code: uploaded.pickCode };
      }
      if (signal.aborted) return;
      const response = await torrentParse(seed.value);
      if (signal.aborted) return;
      if (!response.data.info_hash || !response.data.torrent_filelist.length) {
        throw new Error('种子内没有可下载的文件');
      }
      torrent.value = response.data;
      selectedItems.value = new Set(
        response.data.torrent_filelist.flatMap((file, index) =>
          Number(file.wanted) === 1 ? [String(index)] : [],
        ),
      );
      page.value = 1;
      emit('ready');
    } catch (error) {
      if (!signal.aborted) {
        errorMessage.value = (error as { message?: string }).message || '种子解析失败，请重试';
      }
    } finally {
      if (!signal.aborted) busy.value = false;
    }
  }

  async function handleChoose() {
    if (busy.value || props.disabled) return;
    busy.value = true;
    let path: string | string[] | null = null;
    try {
      path = await open({
        title: '选择种子文件',
        multiple: false,
        filters: [{ name: 'BT种子', extensions: ['torrent'] }],
      });
    } catch (error) {
      if (!disposed) errorMessage.value = '选择种子文件失败，请重试';
      console.error(error);
    } finally {
      if (!disposed) busy.value = false;
    }
    if (disposed || typeof path !== 'string') return;
    if (!path.toLowerCase().endsWith('.torrent')) {
      errorMessage.value = '请选择种子文件';
      return;
    }
    filePath.value = path;
    fileName.value = path.split(/[\\/]/).pop() || path;
    uploadId.value = undefined;
    seed.value = undefined;
    // busy 是父组件传入的 v-model，等父组件更新后再检查，避免仍读到选择文件时的 true。
    await nextTick();
    if (!disposed) await prepare();
  }

  const status = computed(() => ({
    errorMessage: errorMessage.value,
    canRetry: Boolean(filePath.value),
  }));
  defineExpose({ handleChoose, prepare, status });

  onScopeDispose(() => {
    disposed = true;
    controller?.abort();
    task.value = null;
    busy.value = false;
  });
</script>
