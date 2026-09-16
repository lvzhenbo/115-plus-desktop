<script setup lang="tsx">
  import { useUserStore, type FavoriteFolder } from '@/store/user';
  import { DeleteOutlined, EditOutlined, FolderOpenOutlined, FolderOutlined } from '@vicons/antd';
  import type { DropdownOption } from 'naive-ui';

  const props = withDefaults(
    defineProps<{
      /** 当前所在目录，用于列表高亮 */
      currentCid?: string;
    }>(),
    {
      currentCid: '0',
    },
  );

  const collapsed = defineModel<boolean>('collapsed', { default: true });

  const userStore = useUserStore();
  const message = useMessage();

  const emit = defineEmits<{
    navigate: [cid: string];
  }>();

  // ========== 列表展示 ==========

  /** 最近收藏/更新的在最上 */
  const sortedFavorites = computed(() =>
    [...userStore.favorites].sort((a, b) => b.favoritedAt - a.favoritedAt),
  );

  // ========== 重命名 ==========

  const editingCid = ref<string | null>(null);
  const editingName = ref('');

  function startRename(item: FavoriteFolder) {
    editingCid.value = item.cid;
    editingName.value = item.name;
  }

  function commitRename() {
    const cid = editingCid.value;
    if (!cid) return;

    const name = editingName.value.trim();
    const target = userStore.favorites.find((f) => f.cid === cid);
    editingCid.value = null;

    if (name && target && name !== target.name) {
      userStore.renameFavorite(cid, name);
    }
  }

  // ========== 右键菜单 ==========

  const menuVisible = ref(false);
  const menuX = ref(0);
  const menuY = ref(0);
  const menuTarget = ref<FavoriteFolder | null>(null);

  const menuOptions = computed<DropdownOption[]>(() => [
    {
      label: '打开',
      key: 'open',
      icon: () => (
        <NIcon>
          <FolderOpenOutlined />
        </NIcon>
      ),
    },
    {
      label: '重命名',
      key: 'rename',
      icon: () => (
        <NIcon>
          <EditOutlined />
        </NIcon>
      ),
    },
    { type: 'divider', key: 'd1' },
    {
      label: () => <NText type="error">移除</NText>,
      key: 'remove',
      icon: () => (
        <NIcon color="var(--error-color)">
          <DeleteOutlined />
        </NIcon>
      ),
    },
  ]);

  function openMenu(item: FavoriteFolder, event: MouseEvent) {
    menuTarget.value = item;
    menuX.value = event.clientX;
    menuY.value = event.clientY;
    menuVisible.value = true;
  }

  function handleMenuSelect(key: string) {
    const item = menuTarget.value;
    menuVisible.value = false;
    if (!item) return;

    if (key === 'open') handleClick(item);
    else if (key === 'rename') startRename(item);
    else if (key === 'remove') handleRemove(item);
  }

  // ========== 基础操作 ==========

  function handleClick(item: FavoriteFolder) {
    emit('navigate', item.cid);
  }

  function handleRemove(item: FavoriteFolder) {
    userStore.removeFavorite(item.cid);
    message.success('已从收藏夹移除');
  }
</script>

<template>
  <Transition
    enter-active-class="transition-all duration-200"
    leave-active-class="transition-all duration-200"
    enter-from-class="w-0!"
    leave-to-class="w-0!"
  >
    <div v-if="!collapsed" class="flex justify-end overflow-hidden shrink-0 w-44">
      <div class="flex flex-col w-44 shrink-0 bg-(--action-color) border-l border-(--border-color)">
        <div
          class="flex items-center px-3 py-1.5 text-sm font-medium border-b border-(--border-color) text-(--text-color-3)"
        >
          收藏夹
        </div>

        <NScrollbar v-if="sortedFavorites.length > 0" class="flex-1">
          <TransitionGroup
            appear
            enter-active-class="transition-all duration-200"
            leave-active-class="transition-all duration-200"
            move-class="transition-all duration-200"
            enter-from-class="opacity-0 -translate-x-1.5"
            leave-to-class="opacity-0 -translate-x-1.5"
          >
            <div
              v-for="item in sortedFavorites"
              :key="item.cid"
              class="flex items-center px-2 py-1.5 text-xs gap-1.5 group hover:bg-(--primary-color)/10 cursor-pointer"
              :class="{ 'bg-(--primary-color)/15': item.cid === props.currentCid }"
              @click="handleClick(item)"
              @contextmenu.prevent="openMenu(item, $event)"
            >
              <NIcon size="14" class="shrink-0 text-(--warning-color)">
                <FolderOutlined />
              </NIcon>

              <NInput
                v-if="editingCid === item.cid"
                v-model:value="editingName"
                size="tiny"
                autofocus
                class="flex-1 min-w-0"
                @click.stop
                @keydown.enter="commitRename"
                @keydown.esc="editingCid = null"
                @blur="commitRename"
              />
              <NEllipsis v-else class="flex-1 min-w-0" :tooltip="{ placement: 'right' }">
                {{ item.name }}
              </NEllipsis>

              <NButton
                text
                size="tiny"
                type="error"
                class="shrink-0 opacity-0 group-hover:opacity-100 transition-opacity"
                @click.stop="handleRemove(item)"
              >
                <template #icon>
                  <NIcon>
                    <DeleteOutlined />
                  </NIcon>
                </template>
              </NButton>
            </div>
          </TransitionGroup>
        </NScrollbar>

        <NEmpty v-else description="暂无收藏" class="flex-1 flex justify-center items-center" />
      </div>
    </div>
  </Transition>

  <NDropdown
    :show="menuVisible"
    :x="menuX"
    :y="menuY"
    :options="menuOptions"
    placement="bottom-start"
    @select="handleMenuSelect"
    @clickoutside="menuVisible = false"
  />
</template>
