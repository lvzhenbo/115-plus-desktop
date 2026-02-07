<template>
  <NDrawer v-model:show="show" :width="420" display-directive="show" :close-on-esc="false">
    <NDrawerContent closable :native-scrollbar="false" body-content-class="p-0! h-full!">
      <template #header>
        <div class="flex items-center gap-2">
          <span>视频列表 ({{ videoList.length }})</span>
          <NButton size="tiny" quaternary @click="scrollToCurrent">
            <template #icon>
              <NIcon><AimOutlined /></NIcon>
            </template>
            定位当前
          </NButton>
        </div>
      </template>
      <NEl tag="div" class="h-full">
        <NVirtualList
          ref="virtualListRef"
          :items="videoList"
          :item-size="42"
          key-field="pc"
          class="h-full"
        >
          <template #default="{ item }: { item: MyFile }">
            <div
              class="flex items-center gap-3 px-3 py-2.5 cursor-pointer rounded-md mx-1 my-0.5 transition-colors"
              :class="
                item.pc === pickCode
                  ? 'bg-(--primary-color)/10! dark:bg-(--primary-color)/15! text-(--primary-color) font-medium'
                  : 'hover:bg-(--hover-color)'
              "
              @click="handleSelect(item.pc)"
            >
              <NIcon size="16" class="shrink-0">
                <PlayCircleOutlined v-if="item.pc === pickCode" />
                <VideoCameraOutlined v-else />
              </NIcon>
              <NEllipsis :tooltip="{ width: 300 }" class="text-sm flex-1">
                {{ item.fn }}
              </NEllipsis>
            </div>
          </template>
        </NVirtualList>
      </NEl>
    </NDrawerContent>
  </NDrawer>
</template>

<script setup lang="ts">
  import type { MyFile } from '@/api/types/file';
  import { PlayCircleOutlined, VideoCameraOutlined, AimOutlined } from '@vicons/antd';
  import type { VirtualListInst } from 'naive-ui';

  const show = defineModel('show', {
    type: Boolean,
    default: false,
  });

  const pickCode = defineModel('pickCode', {
    type: String,
    default: '',
  });

  defineProps<{
    videoList: MyFile[];
  }>();

  const virtualListRef = ref<VirtualListInst | null>(null);

  const handleSelect = (pc: string) => {
    pickCode.value = pc;
  };

  const scrollToCurrent = () => {
    virtualListRef.value?.scrollTo({ key: pickCode.value, behavior: 'smooth' });
  };

  // 打开时自动滚动到当前播放项
  watch(show, (visible) => {
    if (!visible) return;
    nextTick(scrollToCurrent);
  });
</script>
