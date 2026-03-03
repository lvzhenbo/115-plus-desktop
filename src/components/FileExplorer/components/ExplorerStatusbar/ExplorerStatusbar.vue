<script setup lang="ts">
  defineProps<{
    totalCount: number;
    selectedCount: number;
    currentPage: number;
    totalPages: number;
    pageSize: number;
  }>();

  const emit = defineEmits<{
    'update:currentPage': [page: number];
    'update:pageSize': [size: number];
  }>();
</script>

<template>
  <div
    class="flex items-center justify-between p-3 border-t shrink-0 text-(--text-color-3) border-(--border-color) bg-(--action-color)"
  >
    <div class="flex items-center gap-3">
      <span>共 {{ totalCount }} 项</span>
      <span v-if="selectedCount > 0" class="text-(--primary-color)">
        已选择 {{ selectedCount }} 项
      </span>
    </div>
    <div class="flex items-center gap-2">
      <NPagination
        :page="currentPage"
        :page-count="totalPages"
        :page-size="pageSize"
        :page-sizes="[20, 50, 100, 200]"
        show-size-picker
        show-quick-jumper
        @update:page="emit('update:currentPage', $event)"
        @update:page-size="emit('update:pageSize', $event)"
      />
    </div>
  </div>
</template>
