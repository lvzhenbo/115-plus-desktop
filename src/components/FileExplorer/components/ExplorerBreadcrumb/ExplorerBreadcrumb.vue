<script setup lang="ts">
  import type { Path } from '@/api/types/file';
  import { StarFilled, StarOutlined } from '@vicons/antd';
  import { BookmarksOutlined } from '@vicons/material';

  defineProps<{
    path: Path[];
    loading: boolean;
    /** 当前目录是否已收藏 */
    favorited?: boolean;
    /** 根目录与搜索模式下不提供收藏入口 */
    favoriteDisabled?: boolean;
    /** 是否显示收藏夹面板开关 */
    showFavorites?: boolean;
    /** 收藏夹面板是否展开 */
    favoritesOpen?: boolean;
  }>();

  const emit = defineEmits<{
    navigate: [cid: string];
    toggleFavorite: [];
    toggleFavorites: [];
  }>();
</script>

<template>
  <div
    class="flex items-center px-3 py-1.5 border-b border-(--border-color) bg-(--action-color) min-h-9 gap-2"
  >
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
    <NTooltip>
      <template #trigger>
        <NButton
          quaternary
          circle
          size="small"
          :type="favorited ? 'warning' : 'default'"
          :disabled="favoriteDisabled"
          @click="emit('toggleFavorite')"
        >
          <template #icon>
            <NIcon>
              <StarFilled v-if="favorited" />
              <StarOutlined v-else />
            </NIcon>
          </template>
        </NButton>
      </template>
      {{ favorited ? '取消收藏' : '收藏当前文件夹' }}
    </NTooltip>
    <NTooltip v-if="showFavorites">
      <template #trigger>
        <NButton
          quaternary
          circle
          size="small"
          :type="favoritesOpen ? 'primary' : 'default'"
          @click="emit('toggleFavorites')"
        >
          <template #icon>
            <NIcon>
              <BookmarksOutlined />
            </NIcon>
          </template>
        </NButton>
      </template>
      {{ favoritesOpen ? '收起收藏夹' : '展开收藏夹' }}
    </NTooltip>
  </div>
</template>
