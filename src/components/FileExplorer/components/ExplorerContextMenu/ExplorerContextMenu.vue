<script setup lang="tsx">
  import {
    FolderOpenOutlined,
    InfoCircleOutlined,
    ReloadOutlined,
    CopyOutlined,
    DeleteOutlined,
    DownloadOutlined,
    UploadOutlined,
    OrderedListOutlined,
    StarOutlined,
    StarFilled,
  } from '@vicons/antd';
  import { DriveFileMoveOutlined, DriveFileRenameOutlineOutlined } from '@vicons/material';
  import { useUserStore } from '@/store/user';
  import type { DropdownOption } from 'naive-ui';
  import type { MyFile } from '@/api/types/file';
  import type { ContextMenuAction } from '../../types';

  const props = defineProps<{
    visible: boolean;
    x: number;
    y: number;
    targetItem: MyFile | null;
    /** 当前选中项数量：0 = 空白处右键，1 = 单文件菜单，>1 = 多选菜单 */
    selectedCount: number;
    show: ContextMenuAction[];
  }>();

  const emit = defineEmits<{
    close: [];
    open: [];
    reload: [];
    download: [];
    uploadFile: [];
    rename: [];
    batchRename: [];
    copy: [];
    move: [];
    delete: [];
    detail: [];
  }>();

  const themeVars = useThemeVars();
  const userStore = useUserStore();
  const message = useMessage();

  const isTargetFavorited = computed(() => {
    if (!props.targetItem || props.targetItem.fc !== '0') return false;
    return userStore.isFavorited(props.targetItem.fid);
  });

  const fileMenuOptions = computed<DropdownOption[]>(() => {
    const items: DropdownOption[] = [
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
        label: '刷新',
        key: 'reload',
        icon: () => (
          <NIcon>
            <ReloadOutlined />
          </NIcon>
        ),
      },
      { type: 'divider', key: 'd1' },
      {
        label: '下载',
        key: 'download',
        icon: () => (
          <NIcon>
            <DownloadOutlined />
          </NIcon>
        ),
      },
      {
        label: '上传文件',
        key: 'uploadFile',
        icon: () => (
          <NIcon>
            <UploadOutlined />
          </NIcon>
        ),
      },
      { type: 'divider', key: 'd2' },
      {
        label: '复制到',
        key: 'copy',
        icon: () => (
          <NIcon>
            <CopyOutlined />
          </NIcon>
        ),
      },
      {
        label: '移动到',
        key: 'move',
        icon: () => (
          <NIcon>
            <DriveFileMoveOutlined />
          </NIcon>
        ),
      },
      {
        label: '重命名',
        key: 'rename',
        icon: () => (
          <NIcon>
            <DriveFileRenameOutlineOutlined />
          </NIcon>
        ),
      },
      {
        label: '批量重命名',
        key: 'batchRename',
        disabled: props.selectedCount === 0,
        icon: () => (
          <NIcon>
            <OrderedListOutlined />
          </NIcon>
        ),
      },
      ...(props.targetItem?.fc === '0'
        ? [
            { type: 'divider', key: 'd3' },
            {
              label: isTargetFavorited.value ? '取消收藏' : '添加到收藏夹',
              key: 'toggleFavorite',
              icon: () => (
                <NIcon color={isTargetFavorited.value ? 'var(--warning-color)' : undefined}>
                  {isTargetFavorited.value ? <StarFilled /> : <StarOutlined />}
                </NIcon>
              ),
            },
          ]
        : []),
      { type: 'divider', key: 'd4' },
      {
        label: '详情',
        key: 'detail',
        icon: () => (
          <NIcon>
            <InfoCircleOutlined />
          </NIcon>
        ),
      },
      {
        label: () => <NText type="error">删除</NText>,
        key: 'delete',
        icon: () => (
          <NIcon color={themeVars.value.errorColor}>
            <DeleteOutlined />
          </NIcon>
        ),
      },
    ];

    return normalizeOptions(items);
  });

  /**
   * 多选（>1）时的菜单：只保留可作用于整个选区的项，
   * 打开 / 重命名 / 详情 / 收藏等单文件项不展示，避免"高亮多个却只操作一个"。
   */
  const multiMenuOptions = computed<DropdownOption[]>(() => {
    const count = props.selectedCount;
    const items: DropdownOption[] = [
      {
        label: '刷新',
        key: 'reload',
        icon: () => (
          <NIcon>
            <ReloadOutlined />
          </NIcon>
        ),
      },
      {
        label: '上传文件',
        key: 'uploadFile',
        icon: () => (
          <NIcon>
            <UploadOutlined />
          </NIcon>
        ),
      },
      { type: 'divider', key: 'd1' },
      {
        label: `下载 (${count})`,
        key: 'download',
        icon: () => (
          <NIcon>
            <DownloadOutlined />
          </NIcon>
        ),
      },
      {
        label: `复制到 (${count})`,
        key: 'copy',
        icon: () => (
          <NIcon>
            <CopyOutlined />
          </NIcon>
        ),
      },
      {
        label: `移动到 (${count})`,
        key: 'move',
        icon: () => (
          <NIcon>
            <DriveFileMoveOutlined />
          </NIcon>
        ),
      },
      {
        label: `批量重命名 (${count})`,
        key: 'batchRename',
        icon: () => (
          <NIcon>
            <OrderedListOutlined />
          </NIcon>
        ),
      },
      { type: 'divider', key: 'd2' },
      {
        label: () => <NText type="error">{`删除 (${count})`}</NText>,
        key: 'delete',
        icon: () => (
          <NIcon color={themeVars.value.errorColor}>
            <DeleteOutlined />
          </NIcon>
        ),
      },
    ];
    return normalizeOptions(items);
  });

  /** 按 show 过滤菜单项，并移除首尾 / 连续的分割线 */
  function normalizeOptions(items: DropdownOption[]): DropdownOption[] {
    const filtered = items.filter(
      (item) => item.type === 'divider' || props.show.includes(item.key as ContextMenuAction),
    );

    const result: DropdownOption[] = [];
    for (const item of filtered) {
      if (item.type === 'divider') {
        if (result.length > 0 && result[result.length - 1]?.type !== 'divider') {
          result.push(item);
        }
      } else {
        result.push(item);
      }
    }
    if (result.length > 0 && result[result.length - 1]?.type === 'divider') {
      result.pop();
    }
    return result;
  }

  const bgMenuOptions = computed<DropdownOption[]>(() => {
    const items: DropdownOption[] = [
      {
        label: '刷新',
        key: 'reload',
        icon: () => (
          <NIcon>
            <ReloadOutlined />
          </NIcon>
        ),
      },
      {
        label: '上传文件',
        key: 'uploadFile',
        icon: () => (
          <NIcon>
            <UploadOutlined />
          </NIcon>
        ),
      },
    ];
    return items.filter((item) => props.show.includes(item.key as ContextMenuAction));
  });

  const options = computed(() => {
    if (!props.targetItem) return bgMenuOptions.value;
    if (props.selectedCount > 1) return multiMenuOptions.value;
    return fileMenuOptions.value;
  });

  function handleSelect(key: string) {
    const eventMap: Record<string, () => void> = {
      open: () => emit('open'),
      reload: () => emit('reload'),
      download: () => emit('download'),
      uploadFile: () => emit('uploadFile'),
      copy: () => emit('copy'),
      move: () => emit('move'),
      rename: () => emit('rename'),
      batchRename: () => emit('batchRename'),
      toggleFavorite: () => {
        if (!props.targetItem || props.targetItem.fc !== '0') return;
        if (isTargetFavorited.value) {
          userStore.removeFavorite(props.targetItem.fid);
          message.success('已取消收藏');
        } else {
          userStore.addFavorite(props.targetItem.fid, props.targetItem.fn, props.targetItem.pid);
          message.success('已添加到收藏夹');
        }
      },
      detail: () => emit('detail'),
      delete: () => emit('delete'),
    };
    eventMap[key]?.();
    emit('close');
  }

  function handleClickOutside() {
    emit('close');
  }
</script>

<template>
  <NDropdown
    :show="visible"
    :x="x"
    :y="y"
    :options="options"
    placement="bottom-start"
    trigger="manual"
    @select="handleSelect"
    @clickoutside="handleClickOutside"
  />
</template>
