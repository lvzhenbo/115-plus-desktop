<script setup lang="ts">
  import { filesize } from 'filesize';
  import { format } from 'date-fns';
  import type { MyFile } from '@/api/types/file';
  import { getFileIcon } from '../../types';
  import type { ListColumn } from '../../types';

  const props = defineProps<{
    item: MyFile;
    selected: boolean;
    viewMode: 'grid' | 'list';
    showCheckbox: boolean;
    columns: ListColumn[];
  }>();

  const emit = defineEmits<{
    click: [item: MyFile, event: MouseEvent];
    dblclick: [item: MyFile];
    contextmenu: [item: MyFile, event: MouseEvent];
    check: [item: MyFile, event: MouseEvent];
  }>();

  const icon = computed(() => getFileIcon(props.item));
  const size = computed(() =>
    props.item.fs ? filesize(props.item.fs, { standard: 'jedec' }) : '',
  );
  const createDate = computed(() =>
    props.item.uppt ? format(new Date(props.item.uppt * 1000), 'yyyy-MM-dd HH:mm:ss') : '',
  );
  const modifyDate = computed(() =>
    props.item.uet ? format(new Date(props.item.uet * 1000), 'yyyy-MM-dd HH:mm:ss') : '',
  );
  const fileType = computed(() => {
    if (props.item.fc === '0') return '文件夹';
    return props.item.ico ? `${props.item.ico}文件` : '文件';
  });

  function handleCheck(e: MouseEvent) {
    e.stopPropagation();
    emit('check', props.item, e);
  }
</script>

<template>
  <!-- 网格视图 -->
  <div
    v-if="viewMode === 'grid'"
    data-file-item
    class="group relative flex flex-col items-center p-3 cursor-pointer transition-all duration-150 select-none rounded-(--border-radius)"
    :class="
      selected
        ? 'bg-(--primary-color)/10 ring-1 ring-inset ring-(--primary-color)/35'
        : 'hover:bg-(--hover-color) active:bg-(--pressed-color)'
    "
    @click.stop="emit('click', item, $event)"
    @dblclick.stop="emit('dblclick', item)"
    @contextmenu.prevent.stop="emit('contextmenu', item, $event)"
  >
    <!-- 复选框 -->
    <div
      v-if="showCheckbox"
      class="absolute top-1 left-1 flex items-center transition-opacity"
      :class="selected ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'"
      @click.stop="handleCheck"
    >
      <NCheckbox :checked="selected" />
    </div>

    <!-- 图标 -->
    <div class="w-16 h-16 flex items-center justify-center mb-2">
      <span class="text-4xl leading-none">{{ icon }}</span>
    </div>

    <!-- 文件名 -->
    <div class="text-xs text-center w-full truncate px-1" :title="item.fn">
      {{ item.fn }}
    </div>

    <!-- 视频标记 -->
    <div v-if="item.isv" class="absolute top-1 right-1 text-[10px] text-(--text-color-3)">🎬</div>
  </div>

  <!-- 列表视图 -->
  <div
    v-else
    data-file-item
    class="group flex items-center px-3 py-1.5 cursor-pointer transition-colors duration-100 select-none border-b border-(--divider-color)"
    :class="
      selected ? 'bg-(--primary-color)/10' : 'hover:bg-(--hover-color) active:bg-(--pressed-color)'
    "
    @click.stop="emit('click', item, $event)"
    @dblclick.stop="emit('dblclick', item)"
    @contextmenu.prevent.stop="emit('contextmenu', item, $event)"
  >
    <!-- 复选框 -->
    <div
      v-if="showCheckbox"
      class="w-6 shrink-0 flex items-center justify-center"
      @click.stop="handleCheck"
    >
      <NCheckbox :checked="selected" />
    </div>

    <!-- 图标 -->
    <div class="w-8 h-8 flex items-center justify-center shrink-0">
      <span class="text-xl leading-none">{{ icon }}</span>
    </div>

    <!-- 名称 -->
    <div class="flex-1 min-w-0 px-2 text-sm truncate" :title="item.fn">
      {{ item.fn }}
    </div>

    <!-- 大小 -->
    <div v-if="columns.includes('size')" class="w-24 text-sm shrink-0 px-2 text-(--text-color-3)">
      {{ item.fc === '0' ? '-' : size }}
    </div>

    <!-- 种类 -->
    <div v-if="columns.includes('type')" class="w-20 text-sm shrink-0 px-2 text-(--text-color-3)">
      {{ fileType }}
    </div>

    <!-- 创建时间 -->
    <div
      v-if="columns.includes('createTime')"
      class="w-40 text-sm shrink-0 px-2 text-(--text-color-3)"
    >
      {{ createDate }}
    </div>

    <!-- 修改时间 -->
    <div
      v-if="columns.includes('modifyTime')"
      class="w-40 text-sm shrink-0 px-2 text-(--text-color-3)"
    >
      {{ modifyDate }}
    </div>
  </div>
</template>
