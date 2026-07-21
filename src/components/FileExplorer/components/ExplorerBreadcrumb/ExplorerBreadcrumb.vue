<script setup lang="ts">
  import type { Path } from '@/api/types/file';
  import { StarFilled } from '@vicons/antd';

  defineProps<{
    path: Path[];
    loading: boolean;
  }>();

  const emit = defineEmits<{
    navigate: [cid: string];
    toggleFavorite: [];
  }>();

  function toggleFavorite() {
    emit('toggleFavorite');
  }
</script>

<template>
  <div
    class="flex items-center px-3 py-1.5 border-b border-(--border-color) bg-(--action-color) min-h-9 gap-2"
  >
    <NButton quaternary circle type="warning" size="small" @click="toggleFavorite">
      <template #icon>
        <NIcon>
          <StarFilled />
        </NIcon>
      </template>
    </NButton>
    <NBreadcrumb class="flex-1">
      <NBreadcrumbItem v-for="item in path" :key="item.cid" @click="emit('navigate', item.cid)">
        <NEllipsis
          class="max-w-60!"
          :tooltip="{
            placement: 'top',
            width: 'trigger',
          }"
        >
          {{ item.name }}
        </NEllipsis>
      </NBreadcrumbItem>
    </NBreadcrumb>
  </div>
</template>
