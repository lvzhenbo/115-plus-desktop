<template>
  <NEl class="flex flex-col h-[calc(100vh-59px)] text-(--text-color-2)">
    <ExplorerToolbarLayout>
      <NTooltip>
        <template #trigger>
          <NButton size="small" quaternary :loading="loading" @click="getTaskList">
            <template #icon
              ><NIcon><ReloadOutlined /></NIcon
            ></template>
          </NButton>
        </template>
        刷新
      </NTooltip>
    </ExplorerToolbarLayout>
    <ListHeader
      v-if="items.length"
      :items
      :selected-items="selectedItems"
      :show-checkbox="false"
      :columns="['size']"
      sort-disabled
    >
      <template #after>
        <div class="w-48 shrink-0 px-2">进度</div>
        <div class="w-40 shrink-0 px-2 whitespace-nowrap">添加时间</div>
        <div class="w-52 shrink-0 px-2">操作</div>
      </template>
    </ListHeader>
    <NEmpty v-if="!items.length && !loading" class="flex-1 flex items-center justify-center" />
    <NSpin v-else :show="loading" class="flex-1 min-h-0 overflow-hidden" content-class="h-full">
      <NScrollbar class="h-full">
        <FileItemView
          v-for="item in items"
          :key="item.fid"
          :item
          :selected="false"
          :show-checkbox="false"
          :columns="['size']"
          view-mode="list"
        >
          <template #name>
            <a
              v-if="canOpen(item.task)"
              href="#"
              class="text-inherit no-underline"
              @click.prevent.stop="handleOpen(item.task)"
              >{{ item.fn }}</a
            >
            <template v-else>{{ item.fn }}</template>
          </template>
          <template #size="{ size }">{{ size }}</template>
          <template #after>
            <div class="w-48 shrink-0 px-2 flex items-center self-stretch">
              <TaskStatus
                :type="item.downloadStatus.type"
                :text="item.downloadStatus.text"
                :title="item.downloadStatus.text"
              />
            </div>
            <div class="w-40 shrink-0 px-2 text-sm whitespace-nowrap text-(--text-color-3)">
              {{
                item.task.add_time
                  ? format(new Date(item.task.add_time * 1000), 'yyyy-MM-dd HH:mm:ss')
                  : ''
              }}
            </div>
            <div class="w-52 shrink-0 px-2 flex items-center gap-3 self-stretch" @click.stop>
              <NButton v-if="canOpen(item.task)" text type="primary" @click="handleOpen(item.task)">
                <template #icon
                  ><NIcon><FolderOutlined /></NIcon
                ></template>
                查看
              </NButton>
              <NButton text type="info" @click="handleCopy(item.task)">
                <template #icon
                  ><NIcon><CopyOutlined /></NIcon
                ></template>
                复制
              </NButton>
              <NButton text type="error" @click="confirmDelete(item.task)">
                <template #icon
                  ><NIcon><DeleteOutlined /></NIcon
                ></template>
                删除
              </NButton>
            </div>
          </template>
        </FileItemView>
      </NScrollbar>
    </NSpin>
    <ExplorerStatusbar
      :total-count="count"
      :selected-count="0"
      :current-page="pagination.page || 1"
      :total-pages="pagination.pageCount || 0"
      :page-size="30"
      :show-size-picker="false"
      @update:current-page="handlePageChange"
    />
  </NEl>
</template>

<script setup lang="ts">
  import { taskDelete, taskList } from '@/api/cloud';
  import TaskStatus, { type TaskStatusType } from '@/components/TaskStatus/TaskStatus.vue';
  import type { Task } from '@/api/types/cloud';
  import { format } from 'date-fns';
  import { NCheckbox, type PaginationProps } from 'naive-ui';
  import FileItemView from '@/components/FileExplorer/components/FileItemView/FileItemView.vue';
  import ExplorerToolbarLayout from '@/components/FileExplorer/components/ExplorerToolbarLayout/ExplorerToolbarLayout.vue';
  import ListHeader from '@/components/FileExplorer/components/ListHeader/ListHeader.vue';
  import ExplorerStatusbar from '@/components/FileExplorer/components/ExplorerStatusbar/ExplorerStatusbar.vue';
  import { ReloadOutlined, CopyOutlined, DeleteOutlined, FolderOutlined } from '@vicons/antd';
  import { useSettingStore } from '@/store/setting';

  const settingStore = useSettingStore();
  const router = useRouter();
  const message = useMessage();
  const dialog = useDialog();
  const { copy } = useClipboard();
  const data = ref<Task[]>([]);
  const loading = ref(false);
  const count = ref(0);
  const selectedItems = new Set<string>();
  const items = computed(() =>
    data.value.map((task) => ({
      fid: task.info_hash,
      fn: task.name,
      fc: String(task.file_category ?? ''),
      ico: task.file_icon || task.name.split('.').pop()?.toLowerCase() || '',
      fs: task.size,
      def2: task.def2,
      isv: task.def2 > 0 ? 1 : undefined,
      downloadStatus: getDownloadStatus(task),
      task,
    })),
  );
  const pagination = reactive<PaginationProps>({
    page: 1,
    pageCount: 0,
  });
  const flag = ref<1 | 0>(0);
  const canOpen = (task: Task) => Boolean(task.file_id && task.file_id !== '0');

  function getDownloadStatus(task: Task): { type: TaskStatusType; text: string } {
    if (task.status === 2 && task.move === 1) {
      return { type: 'success', text: task.status_text || '下载成功' };
    }
    if (task.status === -1 || (task.status === 2 && task.move === -1)) {
      return {
        type: 'failure',
        text: task.status_text || (task.status === 2 ? '空间不足' : '下载失败'),
      };
    }
    const percentage = Math.round(task.display_percent ?? task.percentDone);
    const text =
      task.status === 0
        ? percentage === 100
          ? '99%'
          : '正在分配服务器'
        : task.status === 2
          ? '99%'
          : `${Math.max(0, Math.min(100, percentage))}%`;
    return {
      type: 'downloading',
      text: task.status_text && task.status_text !== '下载中' ? task.status_text : text,
    };
  }

  const handleOpen = (task: Task) => {
    if (!canOpen(task)) return;
    router.push({
      name: 'Home',
      query: {
        fid: String(task.file_category) === '1' ? task.wp_path_id || '0' : task.file_id,
      },
    });
  };

  const handleCopy = async (task: Task) => {
    await copy(task.url);
    message.success('复制成功！');
  };

  const confirmDelete = (task: Task) => {
    flag.value = settingStore.cloudDownloadSetting.deleteSourceFile ? 1 : 0;
    dialog.warning({
      title: '是否确认删除该下载任务？',
      content: () =>
        h(
          NCheckbox,
          {
            checked: flag.value,
            checkedValue: 1,
            uncheckedValue: 0,
            'onUpdate:checked': (value: 1 | 0) => {
              flag.value = value;
            },
          },
          { default: () => '删除源文件' },
        ),
      positiveText: '确定',
      negativeText: '取消',
      onPositiveClick: () => handleDelete(task.info_hash),
    });
  };

  onMounted(() => {
    getTaskList();
  });

  onActivated(() => {
    getTaskList();
  });

  let listRequest = 0;
  const getTaskList = async () => {
    const request = ++listRequest;
    try {
      loading.value = true;
      const res = await taskList({
        page: pagination.page!,
      });
      if (request !== listRequest) return;
      data.value = res.data.tasks;
      count.value = res.data.count;
      pagination.pageCount = res.data.page_count;
    } catch (error) {
      console.error('Error fetching task list:', error);
    } finally {
      if (request === listRequest) loading.value = false;
    }
  };

  const handlePageChange = (page: number) => {
    pagination.page = page;
    getTaskList();
  };

  const handleDelete = async (info_hash: string) => {
    try {
      await taskDelete({
        info_hash,
        del_source_file: flag.value,
      });
      message.success('删除成功');
      getTaskList();
    } catch (error) {
      console.error('Error deleting task:', error);
    }
  };
</script>
