<script lang="ts">
  export type TaskStatusType = 'analysis' | 'downloading' | 'success' | 'failure';
</script>

<script setup lang="ts">
  defineProps<{ type: TaskStatusType; text: string }>();
</script>

<template>
  <span
    class="task-status flex min-w-0 max-w-full items-center gap-1 text-sm leading-5"
    :class="type"
  >
    <span class="min-w-0 truncate">{{ text }}</span>
  </span>
</template>

<style scoped>
  .task-status {
    color: #a4a9ae;
  }

  /* 115 官方云下载状态图标，按原始 14px 切片显示。 */
  .task-status::before {
    content: '';
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    background: url('@/assets/cloud-download-status.svg') no-repeat -14px 0;
  }

  .task-status.analysis {
    color: #2777f8;
  }

  .task-status.analysis::before {
    background-position-x: 0;
  }

  .task-status.success {
    color: #00b2cc;
  }

  .task-status.success::before {
    background-position-x: -28px;
  }

  .task-status.failure {
    color: #ff4823;
  }

  .task-status.failure::before {
    background-position-x: -42px;
  }
</style>
