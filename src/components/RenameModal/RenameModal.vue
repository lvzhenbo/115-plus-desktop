<template>
  <NModal v-model:show="show" preset="card" class="w-150!" title="重命名">
    <NInputGroup>
      <NInput
        v-model:value="name"
        maxlength="255"
        :allow-input="noSideSpace"
        placeholder="请输入名称"
        clearable
      />
      <NButton type="primary" @click="name = rawName">重置</NButton>
    </NInputGroup>
    <template #action>
      <NSpace justify="end">
        <NButton @click="show = false">取消</NButton>
        <NButton type="primary" @click="handleSubmit">确定</NButton>
      </NSpace>
    </template>
  </NModal>
</template>

<script setup lang="ts">
  import { updateFile } from '@/api/file';
  import type { MyFile } from '@/api/types/file';

  const show = defineModel('show', {
    type: Boolean,
    default: false,
  });

  const props = defineProps<{
    file: MyFile | null;
  }>();

  const emits = defineEmits(['success']);

  const name = ref('');
  const rawName = computed(() => props.file?.fn || '');
  const message = useMessage();

  watch(show, (val) => {
    if (!val) {
      name.value = '';
    } else {
      name.value = props.file?.fn || '';
    }
  });

  const handleSubmit = async () => {
    if (!name.value) {
      message.error('名称不能为空');
      return;
    }
    try {
      await updateFile({
        file_id: props.file!.fid,
        file_name: name.value,
      });
      message.success('重命名成功');
      emits('success');
      show.value = false;
    } catch (error) {
      console.error(error);
    }
  };

  const noSideSpace = (value: string) => !value.startsWith(' ') && !value.endsWith(' ');
</script>
