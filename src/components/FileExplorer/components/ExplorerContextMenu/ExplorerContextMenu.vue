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
  } from '@vicons/antd';
  import { DriveFileMoveOutlined, DriveFileRenameOutlineOutlined } from '@vicons/material';
  import type { DropdownOption } from 'naive-ui';
  import type { MyFile } from '@/api/types/file';
  import type { ContextMenuAction } from '../../types';

  const props = defineProps<{
    visible: boolean;
    x: number;
    y: number;
    targetItem: MyFile | null;
    hasSelection: boolean;
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
        disabled: !props.hasSelection,
        icon: () => (
          <NIcon>
            <OrderedListOutlined />
          </NIcon>
        ),
      },
      { type: 'divider', key: 'd3' },
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

    // 根据 show 过滤菜单项
    const filtered = items.filter(
      (item) => item.type === 'divider' || props.show.includes(item.key as ContextMenuAction),
    );

    // 移除首尾和连续的分割线
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
  });

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

  const options = computed(() => (props.targetItem ? fileMenuOptions.value : bgMenuOptions.value));

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
