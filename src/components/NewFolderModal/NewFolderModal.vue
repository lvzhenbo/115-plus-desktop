<template>
  <NModal v-model:show="show" preset="card" class="w-100!" title="新建文件夹">
    <NInput
      v-model:value="name"
      maxlength="255"
      :allow-input="noSideSpace"
      placeholder="请输入文件夹名称"
    />
    <template #action>
      <NSpace justify="end">
        <NButton @click="show = false">取消</NButton>
        <NButton type="primary" @click="handleSubmit">确定</NButton>
      </NSpace>
    </template>
  </NModal>
</template>

<script setup lang="ts">
  import { addFolder } from '@/api/file';

  const show = defineModel('show', {
    type: Boolean,
    default: false,
  });

  const props = defineProps<{
    pid: string;
  }>();

  const emits = defineEmits(['success']);

  const name = ref('');
  const message = useMessage();

  watch(show, (val) => {
    if (!val) {
      name.value = '';
    }
  });

  const handleSubmit = async () => {
    if (!name.value) {
      message.error('文件夹名称不能为空');
      return;
    }
    try {
      await addFolder({
        pid: props.pid,
        file_name: name.value,
      });
      message.success('新建成功');
      emits('success');
      show.value = false;
    } catch (error) {
      console.error(error);
    }
  };

  const noSideSpace = (value: string) => !value.startsWith(' ') && !value.endsWith(' ');
</script>

<style scoped></style>
