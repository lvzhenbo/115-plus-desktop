<script setup lang="ts">
  import type { MyFile, SortField } from '@/api/types/file';
  import type { SortConfig, ListColumn } from '../../types';

  defineProps<{
    items: MyFile[];
    selectedItems: Set<string>;
    sortConfig: SortConfig;
    showCheckbox: boolean;
    columns: ListColumn[];
  }>();

  const emit = defineEmits<{
    sort: [field: SortField];
    toggleSelectAll: [];
  }>();
</script>

<template>
  <div
    class="flex items-center px-3 py-1.5 text-sm font-medium border-b sticky top-0 z-10 text-(--text-color-3) border-(--border-color) bg-(--card-color)"
  >
    <div
      v-if="showCheckbox"
      class="w-6 shrink-0 flex items-center justify-center"
      @click.stop="emit('toggleSelectAll')"
    >
      <NCheckbox
        :checked="items.length > 0 && items.every((i) => selectedItems.has(i.fid))"
        :indeterminate="
          items.some((i) => selectedItems.has(i.fid)) &&
          !items.every((i) => selectedItems.has(i.fid))
        "
      />
    </div>
    <div
      class="flex-1 min-w-0 px-2 cursor-pointer select-none hover:text-(--text-color-2)"
      @click.stop="emit('sort', 'file_name')"
    >
      名称
      <span v-if="sortConfig.field === 'file_name'" class="ml-1">
        {{ sortConfig.direction === 'asc' ? '↑' : '↓' }}
      </span>
    </div>
    <div
      v-if="columns.includes('size')"
      class="w-24 shrink-0 px-2 cursor-pointer select-none hover:text-(--text-color-2)"
      @click.stop="emit('sort', 'file_size')"
    >
      大小
      <span v-if="sortConfig.field === 'file_size'" class="ml-1">
        {{ sortConfig.direction === 'asc' ? '↑' : '↓' }}
      </span>
    </div>
    <div
      v-if="columns.includes('type')"
      class="w-20 shrink-0 px-2 cursor-pointer select-none hover:text-(--text-color-2)"
      @click.stop="emit('sort', 'file_type')"
    >
      种类
      <span v-if="sortConfig.field === 'file_type'" class="ml-1">
        {{ sortConfig.direction === 'asc' ? '↑' : '↓' }}
      </span>
    </div>
    <div
      v-if="columns.includes('createTime')"
      class="w-40 shrink-0 px-2 cursor-pointer select-none hover:text-(--text-color-2)"
      @click.stop="emit('sort', 'user_ptime')"
    >
      创建时间
      <span v-if="sortConfig.field === 'user_ptime'" class="ml-1">
        {{ sortConfig.direction === 'asc' ? '↑' : '↓' }}
      </span>
    </div>
    <div
      v-if="columns.includes('modifyTime')"
      class="w-40 shrink-0 px-2 cursor-pointer select-none hover:text-(--text-color-2)"
      @click.stop="emit('sort', 'user_utime')"
    >
      修改时间
      <span v-if="sortConfig.field === 'user_utime'" class="ml-1">
        {{ sortConfig.direction === 'asc' ? '↑' : '↓' }}
      </span>
    </div>
  </div>
</template>
