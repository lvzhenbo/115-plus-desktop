<script setup lang="ts">
  import { useUserStore } from '@/store/user';
  import { DeleteOutlined } from '@vicons/antd';

  const props = withDefaults(
    defineProps<{
      currentCid?: string;
    }>(),
    {
      currentCid: '0',
    },
  );

  const collapsed = defineModel<boolean>('collapsed', { default: false });

  const userStore = useUserStore();

  const sortedFavorites = computed(() =>
    [...userStore.favorites].sort((a, b) => b.favoritedAt - a.favoritedAt),
  );

  const emit = defineEmits<{
    navigate: [cid: string];
  }>();

  function handleClick(cid: string) {
    emit('navigate', cid);
  }

  function handleDelete(cid: string) {
    userStore.removeFavorite(cid);
  }
</script>

<template>
  <div v-if="!collapsed" class="flex flex-col bg-(--action-color) min-w-0 shrink-0 w-44">
    <NScrollbar v-if="sortedFavorites.length > 0" class="flex-1">
      <div
        v-for="item in sortedFavorites"
        :key="item.cid"
        class="flex items-center px-2 py-1.5 cursor-pointer hover:bg-(--primary-color)/10 text-xs gap-1.5 truncate group"
        :class="{ 'bg-(--primary-color)/15': item.cid === props.currentCid }"
        @click="handleClick(item.cid)"
      >
        <NEl tag="span" class="shrink-0">📁</NEl>
        <NEl tag="span" class="truncate flex-1 min-w-0">{{ item.name }}</NEl>
        <NButton
          text
          size="tiny"
          type="error"
          class="shrink-0 opacity-0 group-hover:opacity-100 transition-opacity ml-auto"
          @click.stop="handleDelete(item.cid)"
        >
          <NIcon>
            <DeleteOutlined />
          </NIcon>
        </NButton>
      </div>
    </NScrollbar>
    <NEmpty v-else description="暂无收藏" class="flex-1 flex justify-center items-center" />
  </div>
</template>
