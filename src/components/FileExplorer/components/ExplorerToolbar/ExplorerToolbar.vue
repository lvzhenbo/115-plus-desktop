<script setup lang="ts">
  import {
    ReloadOutlined,
    FolderAddOutlined,
    UploadOutlined,
    DownloadOutlined,
    CopyOutlined,
    DeleteOutlined,
    ArrowUpOutlined,
    AppstoreOutlined,
    UnorderedListOutlined,
  } from '@vicons/antd';
  import { DriveFileMoveOutlined } from '@vicons/material';
  import type { DropdownOption } from 'naive-ui';
  import type { ViewMode, ToolbarAction } from '../../types';

  defineProps<{
    viewMode: ViewMode;
    loading: boolean;
    hasSelection: boolean;
    canGoUp: boolean;
    show: ToolbarAction[];
  }>();

  const emit = defineEmits<{
    up: [];
    refresh: [];
    toggleView: [];
    newFolder: [];
    uploadFile: [];
    uploadFolder: [];
    batchDownload: [];
    batchCopy: [];
    batchMove: [];
    batchDelete: [];
  }>();

  const uploadOptions: DropdownOption[] = [
    {
      label: '上传文件',
      key: 'uploadFile',
    },
    {
      label: '上传文件夹',
      key: 'uploadFolder',
    },
  ];

  const handleUploadSelect = (key: string) => {
    if (key === 'uploadFile') {
      emit('uploadFile');
    } else if (key === 'uploadFolder') {
      emit('uploadFolder');
    }
  };
</script>

<template>
  <div class="flex items-center gap-2 px-3 py-2 border-b border-(--border-color) bg-(--card-color)">
    <!-- 返回上一级 -->
    <NTooltip v-if="show.includes('up')">
      <template #trigger>
        <NButton size="small" quaternary :disabled="!canGoUp" @click="emit('up')">
          <template #icon>
            <NIcon>
              <ArrowUpOutlined />
            </NIcon>
          </template>
        </NButton>
      </template>
      上级目录
    </NTooltip>

    <!-- 刷新 -->
    <NTooltip v-if="show.includes('refresh')">
      <template #trigger>
        <NButton size="small" quaternary :loading="loading" @click="emit('refresh')">
          <template #icon>
            <NIcon>
              <ReloadOutlined />
            </NIcon>
          </template>
        </NButton>
      </template>
      刷新
    </NTooltip>

    <!-- 新建文件夹 -->
    <NTooltip v-if="show.includes('newFolder')">
      <template #trigger>
        <NButton size="small" quaternary @click="emit('newFolder')">
          <template #icon>
            <NIcon>
              <FolderAddOutlined />
            </NIcon>
          </template>
        </NButton>
      </template>
      新建文件夹
    </NTooltip>

    <!-- 上传 -->
    <NDropdown v-if="show.includes('upload')" :options="uploadOptions" @select="handleUploadSelect">
      <NButton size="small" quaternary>
        <template #icon>
          <NIcon>
            <UploadOutlined />
          </NIcon>
        </template>
      </NButton>
    </NDropdown>

    <!-- 批量操作按钮 -->
    <NTooltip v-if="show.includes('download')">
      <template #trigger>
        <NButton size="small" quaternary :disabled="!hasSelection" @click="emit('batchDownload')">
          <template #icon>
            <NIcon>
              <DownloadOutlined />
            </NIcon>
          </template>
        </NButton>
      </template>
      下载
    </NTooltip>

    <NTooltip v-if="show.includes('copy')">
      <template #trigger>
        <NButton size="small" quaternary :disabled="!hasSelection" @click="emit('batchCopy')">
          <template #icon>
            <NIcon>
              <CopyOutlined />
            </NIcon>
          </template>
        </NButton>
      </template>
      复制到
    </NTooltip>

    <NTooltip v-if="show.includes('move')">
      <template #trigger>
        <NButton size="small" quaternary :disabled="!hasSelection" @click="emit('batchMove')">
          <template #icon>
            <NIcon>
              <DriveFileMoveOutlined />
            </NIcon>
          </template>
        </NButton>
      </template>
      移动到
    </NTooltip>

    <NTooltip v-if="show.includes('delete')">
      <template #trigger>
        <NButton
          size="small"
          quaternary
          type="error"
          :disabled="!hasSelection"
          @click="emit('batchDelete')"
        >
          <template #icon>
            <NIcon>
              <DeleteOutlined />
            </NIcon>
          </template>
        </NButton>
      </template>
      删除
    </NTooltip>

    <!-- 弹性空间 -->
    <div class="flex-1" />

    <!-- 视图切换 -->
    <NButtonGroup v-if="show.includes('viewToggle')" size="small">
      <NTooltip>
        <template #trigger>
          <NButton
            :type="viewMode === 'grid' ? 'primary' : 'default'"
            quaternary
            @click="viewMode !== 'grid' && emit('toggleView')"
          >
            <template #icon>
              <NIcon>
                <AppstoreOutlined />
              </NIcon>
            </template>
          </NButton>
        </template>
        网格视图
      </NTooltip>

      <NTooltip>
        <template #trigger>
          <NButton
            :type="viewMode === 'list' ? 'primary' : 'default'"
            quaternary
            @click="viewMode !== 'list' && emit('toggleView')"
          >
            <template #icon>
              <NIcon>
                <UnorderedListOutlined />
              </NIcon>
            </template>
          </NButton>
        </template>
        列表视图
      </NTooltip>
    </NButtonGroup>
  </div>
</template>
