<template>
  <NModal v-model:show="show" preset="card" class="w-250!" :title>
    <FileExplorer
      ref="explorerRef"
      v-model:cid="currentCid"
      v-model:view-mode="userStore.folderModalViewMode"
      v-model:sort-config="userStore.folderModalSortConfig"
      only-folder
      :show-checkbox="false"
      :toolbar="['up', 'refresh', 'newFolder', 'viewToggle']"
      :context-menu="['open', 'reload']"
      :columns="['createTime', 'modifyTime']"
      class="h-120!"
    />
    <template #action>
      <div class="flex justify-end">
        <NButton type="primary" :loading="submitting" @click="handleSubmit">{{
          buttonText
        }}</NButton>
      </div>
    </template>
  </NModal>
</template>

<script setup lang="ts">
  import { copyFile, moveFile } from '@/api/file';
  import { useUserStore } from '@/store/user';

  const show = defineModel('show', {
    type: Boolean,
    default: false,
  });

  const props = withDefaults(
    defineProps<{
      type?: 'copy' | 'move' | 'save';
      ids?: string;
    }>(),
    {
      type: 'save',
      ids: '',
    },
  );

  const emits = defineEmits<{
    success: [];
    select: [cid: string];
  }>();

  const title = computed(() => {
    if (props.type === 'copy') return '打开要复制到的目标文件夹';
    if (props.type === 'move') return '打开要移动到的目标文件夹';
    return '选择要保存的目标文件夹';
  });

  const buttonText = computed(() => {
    if (props.type === 'copy') return '复制到这里';
    if (props.type === 'move') return '移动到这里';
    return '保存到这里';
  });

  const userStore = useUserStore();
  const message = useMessage();
  const explorerRef = useTemplateRef('explorerRef');
  const currentCid = ref('0');
  const submitting = ref(false);

  watch(show, (val) => {
    if (val) {
      const cid = userStore.getLatestFolder(props.type);
      if (cid) {
        currentCid.value = cid;
      }
    }
  });

  watch(currentCid, (val) => {
    userStore.setLatestFolder(props.type, val);
  });

  const handleSubmit = async () => {
    if (!currentCid.value) {
      message.error('请选择文件夹');
      return;
    }

    submitting.value = true;
    try {
      if (props.type === 'save') {
        emits('select', currentCid.value);
      } else if (props.type === 'copy') {
        await copyFile({ file_id: props.ids, pid: currentCid.value });
        message.success('复制成功');
        emits('success');
      } else {
        await moveFile({ file_ids: props.ids, to_cid: currentCid.value });
        message.success('移动成功');
        emits('success');
      }
      show.value = false;
    } catch (error) {
      console.error(error);
    } finally {
      submitting.value = false;
    }
  };
</script>
