<template>
  <NDrawer v-model:show="show" :width="502" display-directive="show" :close-on-esc="false">
    <NDrawerContent title="视频列表" closable :native-scrollbar="false">
      <NMenu
        v-model:value="pickCode"
        :options="videoList"
        key-field="pc"
        label-field="fn"
        :render-label="labelRender"
      />
    </NDrawerContent>
  </NDrawer>
</template>

<script setup lang="tsx">
  import type { MyFile } from '@/api/types/file';
  import { type MenuOption, NEllipsis } from 'naive-ui';

  type MyMenuOption = MyFile & MenuOption;

  const show = defineModel('show', {
    type: Boolean,
    default: false,
  });

  const pickCode = defineModel('pickCode', {
    type: String,
    default: '',
  });

  defineProps<{
    videoList: Array<MyMenuOption>;
  }>();

  const labelRender = (option: MenuOption) => {
    return <NEllipsis>{(option as MyMenuOption).fn}</NEllipsis>;
  };
</script>

<style scoped></style>
