<script setup lang="ts">
  import type { MyFile, SortField } from '@/api/types/file';
  import type { ViewMode, SortConfig, ListColumn } from '../../types';

  defineProps<{
    items: MyFile[];
    viewMode: ViewMode;
    loading: boolean;
    selectedItems: Set<string>;
    sortConfig: SortConfig;
    showCheckbox: boolean;
    columns: ListColumn[];
  }>();

  const emit = defineEmits<{
    clickItem: [item: MyFile, event: MouseEvent];
    dblclickItem: [item: MyFile];
    contextmenuItem: [item: MyFile, event: MouseEvent];
    contextmenuBg: [event: MouseEvent];
    sort: [field: SortField];
    clearSelection: [];
    checkItem: [item: MyFile, event: MouseEvent];
    toggleSelectAll: [];
  }>();

  function isSelected(item: MyFile, selectedItems: Set<string>): boolean {
    return selectedItems.has(item.fid);
  }

  function handleBgClick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (!target.closest('[data-file-item]')) {
      emit('clearSelection');
    }
  }

  function handleBgContextMenu(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (!target.closest('[data-file-item]')) {
      emit('contextmenuBg', e);
    }
  }
</script>

<template>
  <div
    class="flex-1 flex flex-col overflow-hidden relative"
    @click="handleBgClick"
    @contextmenu.prevent="handleBgContextMenu"
  >
    <!-- 表头 -->
    <ListHeader
      v-if="items.length > 0"
      :items="items"
      :selected-items="selectedItems"
      :sort-config="sortConfig"
      :show-checkbox="showCheckbox"
      :columns="columns"
      @sort="(field: SortField) => emit('sort', field)"
      @toggle-select-all="emit('toggleSelectAll')"
    />

    <NSpin :show="loading" class="flex-1 overflow-hidden" content-class="h-full">
      <NScrollbar class="h-full">
        <!-- 空状态 -->
        <NEmpty v-if="!loading && items.length === 0" description="当前文件夹为空" class="py-16" />

        <!-- 网格视图 -->
        <div
          v-if="viewMode === 'grid' && items.length > 0"
          class="grid gap-1 p-3"
          style="grid-template-columns: repeat(auto-fill, minmax(100px, 1fr))"
        >
          <FileItemView
            v-for="item in items"
            :key="item.fid"
            :item="item"
            :selected="isSelected(item, selectedItems)"
            :show-checkbox="showCheckbox"
            :columns="columns"
            view-mode="grid"
            @click="(_item: MyFile, e: MouseEvent) => emit('clickItem', _item, e)"
            @dblclick="(_item: MyFile) => emit('dblclickItem', _item)"
            @contextmenu="(_item: MyFile, e: MouseEvent) => emit('contextmenuItem', _item, e)"
            @check="(_item: MyFile, e: MouseEvent) => emit('checkItem', _item, e)"
          />
        </div>

        <!-- 列表视图 -->
        <div v-if="viewMode === 'list' && items.length > 0">
          <FileItemView
            v-for="item in items"
            :key="item.fid"
            :item="item"
            :selected="isSelected(item, selectedItems)"
            :show-checkbox="showCheckbox"
            :columns="columns"
            view-mode="list"
            @click="(_item: MyFile, e: MouseEvent) => emit('clickItem', _item, e)"
            @dblclick="(_item: MyFile) => emit('dblclickItem', _item)"
            @contextmenu="(_item: MyFile, e: MouseEvent) => emit('contextmenuItem', _item, e)"
            @check="(_item: MyFile, e: MouseEvent) => emit('checkItem', _item, e)"
          />
        </div>
      </NScrollbar>
    </NSpin>
  </div>
</template>
